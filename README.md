
# What is it?

Just like vanilla, it's a basic flavour of a tcp chat client. Maybe it will get upnp support? Who knows. For now, it is only useful on the same network. Due to use of `termios`, this will not function on windows.
It has a daemon and a client, with the daemon accepting connections, and notifying the user of them. The client program can then be passed the connection from the daemon and chat. Imagine it as a phone that rings when you have a call, and puts the caller on hold untill you pick up the phone.

# Install

The binaries are called vchat and vchatd.

## Requirements

Due to moving file descriptors over a unix socket, a rust nightly feature is required.
You must be using rust nightly for it to compile

## Steps

You may have to perform `sudo rustup default nightly` to set nightly as the default build for root if you are performing `sudo make install` rather than `make all` then `sudo make install`

`libnotify-dev`

`sudo make install` or (`make all` then `sudo make install`)

Optionaly, you can setup a systemd user service for the daemon to run on.

`./create-service`
`systemctl --user enable --now vchatd.service`
