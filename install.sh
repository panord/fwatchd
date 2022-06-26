#!/bin/sh

if test -d /lib/systemd/system/; then
	install fwatchd.service /lib/systemd/system
fi

if ! id -u fwatch >/dev/null; then
	useradd -r -d / -c "File watching daemon" -s /usr/bin/nologin fwatch
fi

install ./target/release/fwatchctl /usr/sbin
install ./target/release/fwatchd /usr/sbin
