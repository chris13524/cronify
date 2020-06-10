use crontab::Crontab;
use std::convert::TryInto;
use std::env::args;
use std::process::{exit, Command};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn main() {
    // skip the program name
    let mut args = args().skip(1);

    // get the cron expression
    let cron_expression = args
        .next()
        .expect("Expected cron expression as the first argument.");
    let job = Crontab::parse(cron_expression.as_str()).expect("Invalid cron expression.");

    // generate the command
    let command = args.next().expect("Expected command to run.");
    let mut command = Command::new(command);
    for arg in args {
        command.arg(arg);
    }

    // child PID so handler can SIGTERM child
    static CHILD_PID: AtomicI32 = AtomicI32::new(0);
    // flag to set when we want to shut down
    static TERMINATED: AtomicBool = AtomicBool::new(false);

    // Notes on atomic ordering:
    //
    // The terminated flag can have relaxed ordering
    //   This is because if we miss the flag being set (because of relaxed
    //   ordering) we will see it next loop.
    //
    // The child pid must have Acquire Relase ordering
    //   The pid is only read once to kill the child process in the signal
    //   handler so if relaxed memory ordering is used the child
    //   could become orphaned. AcqRel guartine tees that readers (the signal
    //   handler) will see the writes from writers (the main thread). AcqRel
    //   is free on x86_64 because all reads and writes are AcqRel
    //   Note: The child pid used to be a mutex so Acquire Release was the
    //         original memory ordering.
    //

    // create our signal handler
    ctrlc::set_handler(move || {
        // set the TERMINATED flag
        TERMINATED.store(true, Ordering::Relaxed);

        // check if there _is_ a child
        match CHILD_PID.load(Ordering::Acquire) {
            0 => {}
            child => {
                // we have a child!
                use nix::sys::signal::{self, Signal};
                use nix::unistd::Pid;

                // let's SIGTERM it!
                signal::kill(Pid::from_raw(child), Signal::SIGTERM).unwrap();
            }
        };
    })
    .unwrap();

    // keep working while we're not TERMINATED
    while !TERMINATED.load(Ordering::Relaxed) {
        // compute the delay until the next job
        let next = Duration::from_secs(job.find_next_event().unwrap().to_timespec().sec as u64);
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let delay = next - now;

        // convert that into an instant
        let next = Instant::now() + delay;

        // sleep until we've surpasses that; allowing interruption via `TERMINATED` every 500ms
        while next > Instant::now() && !TERMINATED.load(Ordering::Relaxed) {
            sleep(Duration::from_millis(500));
        }
        // if we were interrupted, don't spawn a job
        if TERMINATED.load(Ordering::Relaxed) {
            break;
        }

        // spawn the job
        match command.spawn() {
            Ok(mut process) => {
                // store the child PID
                CHILD_PID.store(
                    process.id().try_into().expect("PID larger than i32"),
                    Ordering::Release,
                );

                // wait for process to stop
                let status = &process.wait().unwrap();

                if !status.success() {
                    match status.code() {
                        Some(code) => {
                            eprintln!("Process exited with code: {}", code);
                        }
                        None => {
                            eprintln!("Processed TERMINATED by signal.");
                        }
                    }
                }

                // delete child PID so signal handler doesn't try to stop an already dead process
                CHILD_PID.store(0, Ordering::Release);
            }
            Err(error) => {
                eprintln!("Could not start process: {}", error);
                exit(1);
            }
        }
    }
}
