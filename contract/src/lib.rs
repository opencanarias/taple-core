
        mod externf;
        mod sdk;
        use serde::{Deserialize, Serialize};
    
        // Intento de simulación de cómo podría ser un contrato
    
        // Definir "estado del sujeto"
        #[repr(C)]
        #[derive(Serialize, Deserialize)]
        pub struct Data {
            pub one: u32,
            pub two: u32,
            pub three: u32,
        }
    
        // Definir "Familia de eventos"
        #[derive(Serialize, Deserialize, Debug)]
        pub enum EventType {
            Notify,
            ModOne{data: u32},
            ModTwo{data: u32},
            ModThree{data: u32},
            ModAll{data: (u32, u32, u32)},
        }
    
        #[no_mangle]
        pub unsafe fn main_function(state_ptr: i32, event_ptr: i32) -> u32 {
            sdk::execute_contract(state_ptr, event_ptr, contract_logic)
        }
    
        // Lógica del contrato con los tipos de datos esperados
        // Devuelve el puntero a los datos escritos con el estado modificado
        fn contract_logic(state: &mut Data, event: &EventType) {
            // Sería posible añadir gestión de errores
            // Podría ser interesante hacer las operaciones directamente como serde_json:Value en lugar de "Custom Data"
            match event {
                EventType::ModAll{data} => {
                    // Evento que modifica el estado entero
                    state.one = data.0;
                    state.two = data.1;
                    state.three = data.2;
                }
                EventType::ModOne{data} => {
                    // Evento que modifica Data.one
                    state.one = *data;
                }
                EventType::ModTwo{data} => {
                    // Evento que modifica Data.two
                    state.two = *data;
                }
                EventType::ModThree{data} => {
                    // Evento que modifica Data.three
                    state.three = *data;
                }
                EventType::Notify => {
                    // Evento que no modifica el estado
                    // Estos eventos se añadirían a la cadena, pero dentro del contrato apenas harían algo
                }
            }
        } 
      