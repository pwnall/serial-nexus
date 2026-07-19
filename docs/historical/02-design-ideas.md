Help me design a daemon + CLI client that manages serial ports.

Please read my initial ideas below. Do some research to see whether any idea
lacks solid technical grounding (example: crate availability). Point out issues
with the high-level design, so we can discuss them and iron them out before we
come to an implementation plan.

The daemon will be written in Rust edition 2024. The daemon absolutely needs to
work on Linux, with bonus points if it works on macOS. I'll take Windows
functionality if it's free (like conditionally using a different crate), but I'm
not willing to add complexity, such as special handling code.

I want to use permissively licensed crates (examples: MIT, Apache 2.0) -- no
crates with restrictive licenses (examples: no commercial use, user or CPU caps)
or copyleft licenses (examples: GPL). I am willing to use command line tools or
daemons that have copyleft licenses, as long as I can use them without any
modifications.

The core functionality is:

* connect to serial ports, pass the data coming from them through the system (to
  elaborated)
* optionally store the data coming from serial ports in log files, which can be
  rotated on demand
* demultiplex data from/to one serial port into multiple pseudo TTYs; admit
  multiple instances of the demultiplexer logic
* multiplex data from multiple serial ports into a stream (Unix or TCP socket);
  one of the demultiplexer logic instances matches this multiplexer

Here is the structure that I thought of so far.

1. The daemon manages a directed acyclic graph. Nodes process data, and get data
   in and out of the system. Edges move data between nodes.

2. Each edge has an upstream node that is closer to the hardware, and a
   downstream node that is closer to the user. The edge points from the upstream
   node to the downstream node. Nodes that write data to files can only be
   downstream nodes. Nodes that create and manage pseudo TTYs can only be
   downstream nodes. Nodes that connect to existing pseudo-TTYs (or serial
   ports) may be upstream or downstream.

3. (Not intuitive) Data can flow in both directions of a directed edge. Example:
   serial port connected to demultiplexer connected to multiple pseudo TTY
   nodes. Each pseudo-TTY should print data coming from the serial port to its
   stream.

4. The directed acyclic graph is exposed as a configuration. The daemon can load
   and apply a configuration (read from a file by the CLI), report its
   configuration (printed and/or written to a file by the CLI), and apply
   configuration changes, such as adding a new serial port or logging serial
   port data to a file.

5. Nodes have attributes. Examples: nodes that log to files have a directory
   storing the logs, a current output file, and the most recent log rotation
   file (serial.log​ went to serial.log.007​), nodes that connect to serial
   ports have connection parameters (baud rate, data bits, parity, stop bits,
   flow control).

6. Nodes have user-visible labels, where a label is a path-like list of
   segments. Examples: a serial port's label is some form of the device name,
   and a demultiplexer adds path segments naming each multiplexed stream.

Here is a non-trivial use case that we can consider while we design. The daemon
connects to a device's serial port, which has a hardware multiplexer. The
configuration demultiplexes the serial port data (requires user knowledge of the
multiplexing protocol), creates pseudo-TTYs for each multiplexed serial stream,
logs each serial stream to a file, then multiplexes all the serial streams back
into one TCP socket that is forwarded to a different computer. On the second
computer, the daemon has a similar configuration. This looks unnecessary, except
that we can connect a second device to the first computer, and now we can
forward all the streams to the second computer. We can do device maintenance
operations by writing to some of the streams on either computer.



