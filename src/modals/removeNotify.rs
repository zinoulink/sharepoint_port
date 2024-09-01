use std::collections::VecDeque;
use std::time::Duration;
use std::thread;

#[derive(Clone)]
struct Notification {
    id: String,
    name: String,
    options: NotificationOptions,
}

#[derive(Clone)]
struct NotificationOptions {
    sticky: bool,
    after: fn(&str, bool),
}

struct RemoveOptions {
    all: bool,
    include_sticky: bool,
    timeout: bool,
}

static mut SP_NOTIFY: Option<VecDeque<Notification>> = None;
static mut SP_NOTIFY_READY: bool = false;

fn remove_notify(name: Option<&str>, options: Option<RemoveOptions>) -> Result<(), String> {
    let options = options.unwrap_or(RemoveOptions {
        all: false,
        include_sticky: true,
        timeout: false,
    });

    // Make sure we are ready
    unsafe {
        if !SP_NOTIFY_READY && SP_NOTIFY.as_ref().unwrap().len() > 0 {
            thread::sleep(Duration::from_millis(150));
            return remove_notify(name, Some(options));
        }
    }

    unsafe {
        if options.all {
            let mut a = VecDeque::new();
            while let Some(notif) = SP_NOTIFY.as_mut().unwrap().pop_front() {
                if !options.include_sticky && notif.options.sticky {
                    a.push_back(notif);
                } else {
                    // Simulating SP.UI.Notify.removeNotification
                    println!("Removing notification: {}", notif.id);
                    let after_fn = notif.options.after;
                    thread::spawn(move || {
                        thread::sleep(Duration::from_millis(150));
                        after_fn(&notif.name, false);
                    });
                }
            }
            *SP_NOTIFY.as_mut().unwrap() = a;
        } else if let Some(name) = name {
            if let Some(index) = SP_NOTIFY.as_ref().unwrap().iter().position(|n| n.name == name) {
                let notif = SP_NOTIFY.as_mut().unwrap().remove(index).unwrap();
                // Simulating SP.UI.Notify.removeNotification
                println!("Removing notification: {}", notif.id);
                let after_fn = notif.options.after;
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(150));
                    after_fn(&notif.name, options.timeout);
                });
            }
        }
    }

    Ok(())
}