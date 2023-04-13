extern "C" { // Host functions
  // Lee el byte del contexto indicado por el puntero
	pub fn read_byte(pointer: i32) -> u8;
  // Obtiene la longitud en bytes de la estructura del contexto que empieza con el puntero indicado
  pub fn pointer_len(pointer: i32) -> i32;
  // Reserva memoria en el estado del contexto para posteriores escrituras
  pub fn alloc(len: u32) -> i32;
  // Escribe un byte en la posición indicada
  pub fn write_byte(ptr: u32, offset: u32, data: u8);
}
