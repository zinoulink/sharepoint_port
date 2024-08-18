use std::collections::VecDeque;
use std::time::{Duration, SystemTime};
use std::sync::{Mutex, Arc};
use std::thread;

struct Options {
    timeout: u64,
    override_all: bool,
    override: bool,
    override_sticky: bool,
    sticky: bool,
    name: u128,
    after: Box<dyn Fn() + Send>,
    fake: bool,
    ignore_queue: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            timeout: 5,
            override_all: false,
            override: false,
            override_sticky: true,
            sticky: false,
            name: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis(),
            after: Box::new(|| {}),
            fake: false,
            ignore_queue: false,
        }
    }
}

struct Notify {
    queue: VecDeque<(String, Options)>,
    notify_list: Vec<NotifyItem>,
    notify_ready: bool,
}

struct NotifyItem {
    name: u128,
    id: u32,
    options: Options,
}

impl Notify {
    fn new() -> Self {
        Notify {
            queue: VecDeque::new(),
            notify_list: vec![],
            notify_ready: false,
        }
    }

    fn add_notification(&mut self, message: String, options: Options) {
        if !self.notify_ready {
            self.queue.push_back((message, options));
            self.load_dependencies();
        } else {
            self.process_queue(options.ignore_queue);
            if !options.fake {
                self.handle_override(&options);
                let id = self.display_notification(&message);
                self.notify_list.push(NotifyItem {
                    name: options.name,
                    id,
                    options,
                });
                self.setup_timeout(&options);
            }
        }
    }

    fn load_dependencies(&mut self) {
        // Simulate loading dependencies
        self.notify_ready = true;
        self.process_queue(false);
    }

    fn process_queue(&mut self, ignore_queue: bool) {
        if !ignore_queue {
            while let Some((msg, mut opt)) = self.queue.pop_front() {
                opt.ignore_queue = true;
                self.add_notification(msg, opt);
            }
        }
    }

    fn handle_override(&mut self, options: &Options) {
        if self.notify_list.is_empty() {
            return;
        }

        if options.override_all {
            self.remove_notify(true, options.override_sticky);
        } else if options.override {
            if let Some(last) = self.notify_list.last() {
                self.remove_notify_by_name(last.name);
            }
        }
    }

    fn display_notification(&self, _message: &str) -> u32 {
        // Simulate adding a notification and returning its ID
        42
    }

    fn remove_notify(&mut self, all: bool, include_sticky: bool) {
        if all {
            self.notify_list.retain(|item| !include_sticky || item.options.sticky);
        }
    }

    fn remove_notify_by_name(&mut self, name: u128) {
        self.notify_list.retain(|item| item.name != name);
    }

    fn setup_timeout(&self, options: &Options) {
        if !options.sticky {
            let name = options.name;
            let timeout = options.timeout;
            let after = options.after.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_secs(timeout));
                (after)();
                // Remove the notification after the timeout
            });
        }
    }
}

fn main() {
    let notify = Arc::new(Mutex::new(Notify::new()));
    let options = Options {
        timeout: 10,
        ..Default::default()
    };

    let message = "This is a test notification".to_string();
    let notify_clone = Arc::clone(&notify);
    let _ = thread::spawn(move || {
        let mut notify = notify_clone.lock().unwrap();
        notify.add_notification(message, options);
    });
}
