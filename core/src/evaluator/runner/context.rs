use std::collections::HashMap;

#[derive(Debug)]
pub struct MemoryManager {
    memory: Vec<u8>,
    map: HashMap<usize, usize>,
}

impl MemoryManager {
    pub fn new() -> Self {
        Self {
            memory: vec![],
            map: HashMap::new(),
        }
    }

    pub fn alloc(&mut self, len: usize) -> usize {
        let current_len = self.memory.len();
        self.memory.resize(current_len + len, 0);
        self.map.insert(current_len, len);
        current_len
    }

    pub fn write_byte(&mut self, start_ptr: usize, offset: usize, data: u8) {
        self.memory[start_ptr + offset] = data;
    }

    pub fn read_byte(&self, ptr: usize) -> u8 {
        self.memory[ptr]
    }

    pub fn read_data(&self, ptr: usize) -> &[u8] {
        let len = self.map.get(&ptr).unwrap();
        &self.memory[ptr..ptr + len]
    }

    pub fn get_pointer_len(&self, ptr: usize) -> usize {
        *self.map.get(&ptr).unwrap()
    }

    pub fn add_date_raw(&mut self, bytes: &[u8]) -> usize {
        let ptr = self.alloc(bytes.len());
        for (index, byte) in bytes.iter().enumerate() {
            self.memory[ptr + index] = *byte;
        }
        ptr
    }
}
