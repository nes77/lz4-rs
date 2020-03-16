use std::collections::{VecDeque, HashMap};

/// Holds pointers to the locations of matches during a compression operation.
/// The VecDeque allows us to remove older entries without needing to scan through the entire map.
#[derive(Debug, Clone)]
pub struct MatchMap<'a> {
    entries: VecDeque<(&'a [u8], usize)>,
    matches: HashMap<&'a [u8], usize>,
    max_match_offset: usize,
}

impl<'a> MatchMap<'a> {
    pub fn new(max_match_offset: usize) -> Self {
        MatchMap { entries: VecDeque::new(), matches: HashMap::new(), max_match_offset }
    }

    pub fn advance(&mut self, new_idx: usize) {
        let cull_idx = new_idx.saturating_sub(self.max_match_offset);
        let elems_to_remove = self.entries.iter()
            .take_while(|(_, wh)| *wh < cull_idx)
            .count();

        let entries = &mut self.entries;
        let matches = &mut self.matches;
        entries.drain(..elems_to_remove)
            .for_each(|f| {
                if matches.contains_key(f.0) {
                    if *matches.get(f.0).unwrap() < cull_idx {
                        matches.remove(f.0);
                    }
                }
            });
    }

    pub fn get_match(&self, item: &[u8]) -> Option<usize> {
        self.matches.get(item).cloned()
    }

    pub fn reset(&mut self) {
        self.matches.clear();
        self.entries.clear();
    }

    pub fn add_prefix<'b : 'a>(&mut self, item: &'b [u8], idx: usize) {
        self.matches.insert(item, idx);
        self.entries.push_back((item, idx));
    }
}