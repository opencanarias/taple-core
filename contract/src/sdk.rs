use crate::externf;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Payload {
    pub current_state: String,
    pub event: String,
}

pub fn execute_contract<F, State, Event>(state_ptr: i32, event_ptr: i32, callback: F) -> u32
where
    State: for<'a> Deserialize<'a> + Serialize,
    Event: for<'a> Deserialize<'a> + Serialize,
    F: Fn(&mut State, &Event),
{
    {
        let mut state: State = serde_json::from_str(&get_from_context(state_ptr)).unwrap();
        let event: Event = serde_json::from_str(&get_from_context(event_ptr)).unwrap();
        callback(&mut state, &event);
        // Después de haber sido modificado debemos guardar el nuevo estado.
        // Sería interesante no tener que guardar estado si el evento no es modificante
        store(&state)
    }
}

fn get_from_context(pointer: i32) -> String {
    let data = unsafe {
        let len = externf::pointer_len(pointer);
        let mut data = vec![];
        for i in 0..len {
            data.push(externf::read_byte(pointer + i));
        }
        data
    };
    String::from_utf8(data).unwrap()
}

pub fn store<State>(data: &State) -> u32
where
    State: for<'a> Deserialize<'a> + Serialize,
{
    // Suponemos que guardamos el JSON String como bytes directamente
    unsafe {
      let data_str = serde_json::to_string(data).unwrap();
      let bytes = data_str.as_bytes();
      let ptr = externf::alloc(bytes.len() as u32) as u32;
      for (index, byte) in bytes.into_iter().enumerate() {
        externf::write_byte(ptr, index as u32, *byte);
      }
      ptr
    }
}
