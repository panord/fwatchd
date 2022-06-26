#!/bin/sh

if test -d /lib/systemd/system/; then
	cp fwatch.service /lib/systemd/system
fi

useradd -r -d / -c "File watching daemon" -s /usr/bin/nologin fwatch

install ./target/release/fwatchctl /usr/sbin
install ./target/release/fwatchd /usr/sbin
