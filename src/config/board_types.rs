//! Board type configuration and management

use crate::models::board::BoardType;
use std::collections::HashMap;
use std::path::PathBuf;

/// Board type registry for managing available board configurations
pub struct BoardTypeRegistry {
    board_types: HashMap<String, BoardType>,
}

impl BoardTypeRegistry {
    pub fn new() -> Self {
        Self {
            board_types: HashMap::new(),
        }
    }

    pub fn add_board_type(&mut self, board_type: BoardType) {
        self.board_types.insert(board_type.id.clone(), board_type);
    }

    pub fn get_board_type(&self, id: &str) -> Option<&BoardType> {
        self.board_types.get(id)
    }

    pub fn list_board_types(&self) -> Vec<&BoardType> {
        self.board_types.values().collect()
    }

    pub fn remove_board_type(&mut self, id: &str) -> Option<BoardType> {
        self.board_types.remove(id)
    }
}

impl Default for BoardTypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}
