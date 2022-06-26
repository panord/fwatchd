# fwatchd

[![Latest Version](https://img.shields.io/crates/v/fwatchd.svg)](https://crates.io/crates/fwatchd/)
[![docs](https://docs.rs/fwatchd/badge.svg)](https://docs.rs/fwatchd)

fwatchd - A file watching daemon

fwatchd can be controlled using fwatchctl, which may instruct fwatchd to track
files and perform some action based on inotify events on that file.


## Create fwatchd user
```bash
usermod -aG fwatch $USER
useradd -r -d / -c "File watching daemon" -s /usr/bin/nologin fwatch
```

## Example Usage
```bash
systemctl enable fwatch
systemctl start fwatch

fwatchctl track /tmp/example
```

## Customizing usage of the event
```bash
fwatchd --foreground

fwatchctl track /tmp/example --alias /usr/bin/echo --script /usr/bin/cat"
echo "test" >> /tmp/example
fwatchctl list
```
