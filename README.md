
# What is it?

Just like vanilla, it's a basic flavour of a tcp chat client. Maybe it will get upnp support? Who knows. For now, it is only useful on the same network. Due to use of `termios`, this will not function on windows.
It has a daemon and a client, with the daemon accepting connections, and notifying the user of them. The client program can then be passed the connection from the daemon and chat. Imagine it as a phone that rings when you have a call, and puts the caller on hold untill you pick up the phone.

# Install

## Requirements

Due to moving file descriptors over a unix socket, a rust nightly feature is required.
You must be using rust nightly for it to compile

`libnotify-dev`

`sudo make install`
