use std::collections::HashSet;
use std::convert::TryInto;
use std::fs::File;
use std::{io, mem, str};

use meilisearch_tokenizer::token::SeparatorKind;
use meilisearch_tokenizer::{Analyzer, AnalyzerConfig, Token, TokenKind};
use roaring::RoaringBitmap;
use serde_json::Value;

use super::helpers::{concat_u32s_array, create_sorter, sorter_into_reader, GrenadParameters};
use crate::error::{InternalError, SerializationError};
use crate::proximity::ONE_ATTRIBUTE;
use crate::{FieldId, Result};

/// Extracts the word and positions where this word appear and
/// prefixes it by the document id.
///
/// Returns the generated internal documents ids and a grenad reader
/// with the list of extracted words from the given chunk of documents.
#[logging_timer::time]
pub fn extract_docid_word_positions<R: io::Read>(
    mut obkv_documents: grenad::Reader<R>,
    indexer: GrenadParameters,
    searchable_fields: &Option<HashSet<FieldId>>,
    stop_words: Option<&fst::Set<&[u8]>>,
) -> Result<(RoaringBitmap, grenad::Reader<File>)> {
    let max_memory = indexer.max_memory_by_thread();

    let mut documents_ids = RoaringBitmap::new();
    let mut docid_word_positions_sorter = create_sorter(
        concat_u32s_array,
        indexer.chunk_compression_type,
        indexer.chunk_compression_level,
        indexer.max_nb_chunks,
        max_memory,
    );

    let mut key_buffer = Vec::new();
    let mut field_buffer = String::new();
    let mut config = AnalyzerConfig::default();
    if let Some(stop_words) = stop_words {
        config.stop_words(stop_words);
    }
    let analyzer = Analyzer::<Vec<u8>>::new(AnalyzerConfig::default());

    while let Some((key, value)) = obkv_documents.next()? {
        let document_id = key
            .try_into()
            .map(u32::from_be_bytes)
            .map_err(|_| SerializationError::InvalidNumberSerialization)?;
        let obkv = obkv::KvReader::<FieldId>::new(value);

        documents_ids.push(document_id);
        key_buffer.clear();
        key_buffer.extend_from_slice(&document_id.to_be_bytes());

        for (field_id, field_bytes) in obkv.iter() {
            if searchable_fields.as_ref().map_or(true, |sf| sf.contains(&field_id)) {
                let value =
                    serde_json::from_slice(field_bytes).map_err(InternalError::SerdeJson)?;
                field_buffer.clear();
                if let Some(field) = json_to_string(&value, &mut field_buffer) {
                    let analyzed = analyzer.analyze(field);
                    let tokens = process_tokens(analyzed.tokens())
                        .take_while(|(p, _)| (*p as u32) < ONE_ATTRIBUTE);

                    for (index, token) in tokens {
                        let token = token.text().trim();
                        if !token.is_empty() {
                            key_buffer.truncate(mem::size_of::<u32>());
                            key_buffer.extend_from_slice(token.as_bytes());

                            let position: u32 = index
                                .try_into()
                                .map_err(|_| SerializationError::InvalidNumberSerialization)?;
                            let position = field_id as u32 * ONE_ATTRIBUTE + position;
                            docid_word_positions_sorter
                                .insert(&key_buffer, &position.to_ne_bytes())?;
                        }
                    }
                }
            }
        }
    }

    sorter_into_reader(docid_word_positions_sorter, indexer).map(|reader| (documents_ids, reader))
}

/// Transform a JSON value into a string that can be indexed.
fn json_to_string<'a>(value: &'a Value, buffer: &'a mut String) -> Option<&'a str> {
    fn inner(value: &Value, output: &mut String) -> bool {
        use std::fmt::Write;
        match value {
            Value::Null => false,
            Value::Bool(boolean) => write!(output, "{}", boolean).is_ok(),
            Value::Number(number) => write!(output, "{}", number).is_ok(),
            Value::String(string) => write!(output, "{}", string).is_ok(),
            Value::Array(array) => {
                let mut count = 0;
                for value in array {
                    if inner(value, output) {
                        output.push_str(". ");
                        count += 1;
                    }
                }
                // check that at least one value was written
                count != 0
            }
            Value::Object(object) => {
                let mut buffer = String::new();
                let mut count = 0;
                for (key, value) in object {
                    buffer.clear();
                    let _ = write!(&mut buffer, "{}: ", key);
                    if inner(value, &mut buffer) {
                        buffer.push_str(". ");
                        // We write the "key: value. " pair only when
                        // we are sure that the value can be written.
                        output.push_str(&buffer);
                        count += 1;
                    }
                }
                // check that at least one value was written
                count != 0
            }
        }
    }

    if let Value::String(string) = value {
        Some(&string)
    } else if inner(value, buffer) {
        Some(buffer)
    } else {
        None
    }
}

/// take an iterator on tokens and compute their relative position depending on separator kinds
/// if it's an `Hard` separator we add an additional relative proximity of 8 between words,
/// else we keep the standart proximity of 1 between words.
fn process_tokens<'a>(
    tokens: impl Iterator<Item = Token<'a>>,
) -> impl Iterator<Item = (usize, Token<'a>)> {
    tokens
        .skip_while(|token| token.is_separator().is_some())
        .scan((0, None), |(offset, prev_kind), token| {
            match token.kind {
                TokenKind::Word | TokenKind::StopWord | TokenKind::Unknown => {
                    *offset += match *prev_kind {
                        Some(TokenKind::Separator(SeparatorKind::Hard)) => 8,
                        Some(_) => 1,
                        None => 0,
                    };
                    *prev_kind = Some(token.kind)
                }
                TokenKind::Separator(SeparatorKind::Hard) => {
                    *prev_kind = Some(token.kind);
                }
                TokenKind::Separator(SeparatorKind::Soft)
                    if *prev_kind != Some(TokenKind::Separator(SeparatorKind::Hard)) =>
                {
                    *prev_kind = Some(token.kind);
                }
                _ => (),
            }
            Some((*offset, token))
        })
        .filter(|(_, t)| t.is_word())
}
