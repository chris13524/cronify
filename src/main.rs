use std::env::args;
use std::time::{SystemTime, UNIX_EPOCH, Duration, Instant};
use std::thread::sleep;
use crontab::Crontab;
use std::process::{Command, exit};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

fn main() {
	// skip the program name
	let mut args = args().skip(1);
	
	// get the cron expression
	let cron_expression = args.next().expect("Expected cron expression as the first argument.");
	let job = Crontab::parse(cron_expression.as_str()).expect("Invalid cron expression.");
	
	// generate the command
	let command = args.next().expect("Expected command to run.");
	let mut command = Command::new(command);
	for arg in args {
		command.arg(arg);
	}
	
	// child PID so handler can SIGTERM child
	let child_pid: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(Option::None));
	// flag to set when we want to shut down
	let terminated = Arc::new(AtomicBool::new(false));
	
	// clone those so we can use them in our signal handler
	let terminated_clone = terminated.clone();
	let child_pid_clone = child_pid.clone();
	
	// create our signal handler
	ctrlc::set_handler(move || {
		// set the terminated flag
		terminated_clone.store(true, Ordering::Relaxed);
		
		// get child PID
		let child = child_pid_clone.lock().unwrap();
		// check if there _is_ a child
		match *child {
			None => {}
			Some(child) => {
				// we have a child!
				use nix::unistd::Pid;
				use nix::sys::signal::{self, Signal};
				
				// let's SIGTERM it!
				signal::kill(Pid::from_raw(child as i32), Signal::SIGTERM).unwrap();
			}
		};
	}).unwrap();
	
	// keep working while we're not terminated
	while !terminated.load(Ordering::Relaxed) {
		// compute the delay until the next job
		let next = Duration::from_secs(job.find_next_event().unwrap().to_timespec().sec as u64);
		let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
		let delay = next - now;
		
		// convert that into an instant
		let next = Instant::now() + delay;
		
		// sleep until we've surpasses that; allowing interruption via `terminated` every 500ms
		while next > Instant::now() && !terminated.load(Ordering::Relaxed) {
			sleep(Duration::from_millis(500));
		}
		// if we were interrupted, don't spawn a job
		if terminated.load(Ordering::Relaxed) { break; }
		
		// spawn the job
		match command.spawn() {
			Ok(mut process) => {
				// store the child PID
				*child_pid.lock().unwrap() = Some(process.id());
				
				// wait for process to stop
				let status = &process.wait().unwrap();
				
				if !status.success() {
					match status.code() {
						Some(code) => {
							eprintln!("Process exited with code: {}", code);
						}
						None => {
							eprintln!("Processed terminated by signal.");
						}
					}
				}
				
				// delete child PID so signal handler doesn't try to stop an already dead process
				*child_pid.lock().unwrap() = None;
			}
			Err(error) => {
				eprintln!("Could not start process: {}", error);
				exit(1);
			}
		}
	}
}
