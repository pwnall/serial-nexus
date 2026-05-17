# Serial Nexus

This is a collection of utilities for working with
[serial ports][wikipedia-serial-port].

The software is written in Rust.

## Use cases

This collection aims to help developers who work with one more devices that
expose debugging consoles via serial ports, which are typically implemented as
[UART connections][wikipedia-uart].

## Concepts

* Target system - logical parent that groups multiple UART connections coming
  from the same hardware

* Host system - the computer that runs the serial nexus software

* Link - logical connection modeled after an UART connection

    * Parameters - configurable knobs in
      [the UART communication protocol][adi-uart-protocol]

    * Configuration - values for all parameters

    * Target end - the end of the connection that behaves like a target system;
      transmits and receives data using fixed parameters

    * Host end - the end of the connection that behaves like the host system;
      transmits and receives data using configurable parameters; the link only
      operates correctly when the host parameters are configured to match the
      target parameters

* Input - manages a UART port coming from a device; exposes input/output data
  streams and configuration (baud rate, etc)

* Node - manages data flowing between an upstream connection and one or more
  downstream connections


### Link parameters

* Voltage level - the voltage level for the logical high (1) signal and for
  the idle (Mark) state; popular values are 1.8V, 3.3V and 5V

* Baud rate - the bit clock frequency in Hz; most popular value is 115,200

* Data bits - number of payload bits (sent between the start bit and the parity
  or stop bit); values are 5-9; the most popular value is 8, with one ASCII
  character per payload

* Parity - optional error-checking bit transmitted after data bit; values are
  "N" (no bit), "E" / "O" (one bit that makes the total number of 1s even / odd)

* Stop bits - duration of the logical high (1) signal transmitted after the
  parity bit or after the last data bit; values are 1, 1.5 and 2; the most
  popular value is 1

* Flow control - method for the receiver to throttle the transmitter; value are
  None, Hardware (RTS / CTS signals), Software (XON / XOFF characters)

UART always uses 1 start bit -- a logical low (0) signal transmitted for 1 bit
clock cycle.

[adi-uart-protocol]: https://www.analog.com/en/resources/analog-dialogue/articles/uart-a-hardware-communication-protocol.html
[wikipedia-serial-port]: https://en.wikipedia.org/wiki/Serial_port
[wikipedia-uart]: https://en.wikipedia.org/wiki/Universal_asynchronous_receiver-transmitter
