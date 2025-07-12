use termios::*;
use std::os::fd::AsRawFd;
use std::io;
use std::io::{Read,Write};
use std::thread;
use std::process::ExitCode;
use std::sync::{Arc,Mutex};
use std::cell::RefCell;

pub struct ThreadedIO {
	io_lock: Mutex<()>,
	input_buffer: Mutex<RefCell<Vec<char>>>,
	current_prompt_state: Mutex<RefCell<String>>,
	old_term_settings: Termios,
}
impl ThreadedIO {
	fn new() -> ThreadedIO{
		let instance = ThreadedIO {
			io_lock: Mutex::new(()),
			input_buffer: Mutex::new(RefCell::new(vec![])),
			current_prompt_state: Mutex::new(RefCell::new("".to_string())),
			old_term_settings: Termios::from_fd(io::stdin().as_raw_fd()).unwrap(),
		};
		//====== setup raw stdin ======
		let mut term = instance.old_term_settings.clone();
		term.c_lflag &= !(ICANON | ECHO); //unbuffered no echo
		term.c_cc[VMIN] = 1; //get at least one byte before read returns
		term.c_cc[VTIME] = 0; //dont wait for bytes
		tcsetattr(io::stdin().as_raw_fd(),TCSANOW,&term).unwrap();
		//return
		instance
	}
	fn println(&self,string: String) -> Result<(),std::io::Error>{
		let _io_guard = self.io_lock.lock();
		let current_prompt_state_binding = self.current_prompt_state.lock().unwrap();
		let current_prompt_state = current_prompt_state_binding.borrow();
		let mut stdout = io::stdout();
		//delete old prompt and insert line
		stdout.write_all(format!("\r\x1b[2K{}\n",string).as_bytes())?;
		//redisplay the prompt
		stdout.write_all(current_prompt_state.as_bytes())?;
		stdout.flush()?;
		Ok(())
	}
	fn input(&self,prompt: &str) -> Result<String,std::io::Error>{
		let input_buffer_binding = self.input_buffer.lock().unwrap();
		let mut input_buffer = input_buffer_binding.borrow_mut();
		{//====== initialy display the prompt ======
			let _io_guard = self.io_lock.lock();
			let current_prompt_state_binding = self.current_prompt_state.lock().unwrap();
			let mut current_prompt_state = current_prompt_state_binding.borrow_mut();
			*current_prompt_state = prompt.to_string() + &input_buffer.iter().collect::<String>();
			let mut stdout = io::stdout();
			stdout.write_all(format!("\r\x1b[2K{}",current_prompt_state).as_bytes())?;
			stdout.flush()?;
		}
		//====== get input bytes ======
		for ch in io::stdin().bytes(){
			match ch?{
				10 => break,//enter
				127 => {input_buffer.pop(); ()}, //delete
				ch => {
					if ch >= 32 && ch <= 126{
						input_buffer.push(char::from(ch));
					}else{
						//self.println(format!("unknown char {}",ch))?;
					}
				},
			}
			//====== display the prompt closure ======
			let _io_guard = self.io_lock.lock();
			let current_prompt_state_binding = self.current_prompt_state.lock().unwrap();
			let mut current_prompt_state = current_prompt_state_binding.borrow_mut();
			*current_prompt_state = prompt.to_string() + &input_buffer.iter().collect::<String>();
			let mut stdout = io::stdout();
			stdout.write_all(format!("\r\x1b[2K{}",current_prompt_state).as_bytes())?;
			stdout.flush()?;
		}
		{//====== clear the input buffer ======
			let _io_guard = self.io_lock.lock();
			let current_prompt_state_binding = self.current_prompt_state.lock().unwrap();
			let mut current_prompt_state = current_prompt_state_binding.borrow_mut();
			*current_prompt_state = "".to_string();
		}
		let message = input_buffer.iter().collect();
		input_buffer.truncate(0);
		Ok(message)
	}
	fn reset_term(&self){
		tcsetattr(io::stdin().as_raw_fd(),TCSANOW,&self.old_term_settings).unwrap();
	}
}
impl Drop for ThreadedIO{
	fn drop(&mut self){
		self.reset_term();
	}
}
fn main() -> ExitCode{
	let threaded_io_instance = ThreadedIO::new();
	let io_controller = Arc::new(threaded_io_instance);
	{//====== receiving messages thread ======
		let io = io_controller.clone();
		thread::spawn(move ||{
		});
	}
	{//====== input handling thread ======
		let io = io_controller.clone();
		match thread::spawn(move ||{
			loop {
				let message = io.input(">>>").unwrap();
				match io.println(format!("you typed: [{}]",message)){
					Err(e) => println!("error {:?}",e),
					Ok(_) => (),
				};
				if message == "/exit" {break;}
			}
		}).join() {
			Ok(_) => ExitCode::SUCCESS,
			Err(e) => {println!("{:?}",e); ExitCode::SUCCESS}
		}
	}
}
