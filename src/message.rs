use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

const FIRST_ID: u64 = 1;

pub const FLAG_IS_FINAL: &str = "is_final";
pub const FLAG_IS_TRANSLATED: &str = "is_translated";

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Message {
 pub id: u64,
 pub from: String,
 pub content: String,
 pub flags: HashSet<String>,
 pub datetime: DateTime<Utc>,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(FIRST_ID);

impl Message {
 pub fn set_next_id(id: u64) {
  NEXT_ID.store(id, Ordering::Relaxed);
 }

 pub fn get_next_id() -> u64 {
  NEXT_ID.load(Ordering::Relaxed)
 }

 pub fn new(from: &str, content: &str) -> Self {
  let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
  Self {
   id,
   from: from.to_string(),
   content: content.to_string(),
   flags: HashSet::new(),
   datetime: Utc::now(),
  }
 }

 pub fn with_flag(mut self, flag: &str) -> Self {
  self.flags.insert(flag.to_string());
  self
 }

 pub fn with_flags(mut self, flags: Vec<String>) -> Self {
  self.flags = flags.into_iter().collect();
  self
 }

 // pub fn add_flag(&mut self, flag: String) {
 //  self.flags.insert(flag);
 // }

 pub fn has_flag(&self, flag: &str) -> bool {
  self.flags.contains(flag)
 }
}
