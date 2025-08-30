#!/usr/bin/env python3
import serial
import socket
import struct
import time
import argparse
import signal
import sys
import re


class TransportBase:
    """Base class for transport implementations"""

    def __init__(self, timeout=0.1):
        self.timeout = timeout

    def write(self, data):
        raise NotImplementedError

    def read(self, size):
        raise NotImplementedError

    def close(self):
        raise NotImplementedError

    def transport_type(self):
        raise NotImplementedError


class SerialTransport(TransportBase):
    """Serial port transport implementation"""

    def __init__(self, port, baudrate=115200, timeout=0.1):
        super().__init__(timeout)
        self.port = serial.Serial(port=port, baudrate=baudrate, timeout=timeout)

    def write(self, data):
        return self.port.write(data)

    def read(self, size):
        return self.port.read(size)

    def close(self):
        if self.port.is_open:
            self.port.close()

    def transport_type(self):
        return "Serial"


class TcpTransport(TransportBase):
    """TCP socket transport implementation"""

    def __init__(self, host, port, timeout=0.25):
        super().__init__(timeout)
        self.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.socket.settimeout(timeout)

        try:
            print(f"Connecting to {host}:{port}...")
            self.socket.connect((host, port))
            print(f"Connected successfully via TCP")
        except Exception as e:
            self.socket.close()
            raise ConnectionError(f"Failed to connect to {host}:{port}: {e}")

    def write(self, data):
        try:
            self.socket.sendall(data)
            return len(data)
        except socket.timeout:
            raise serial.SerialTimeoutException("Write timeout")
        except Exception as e:
            raise IOError(f"TCP write error: {e}")

    def read(self, size):
        try:
            data = b""
            while len(data) < size:
                chunk = self.socket.recv(size - len(data))
                if not chunk:
                    break
                data += chunk
            return data
        except socket.timeout:
            return data  # Return partial data on timeout
        except Exception as e:
            raise IOError(f"TCP read error: {e}")

    def close(self):
        try:
            self.socket.close()
        except:
            pass

    def transport_type(self):
        return "TCP"


def create_transport(target, timeout=None):
    """Create appropriate transport based on target format"""

    # Check if target looks like IP:port
    tcp_pattern = r'^(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}):(\d+)$'
    tcp_ipv6_pattern = r'^\[([^\]]+)\]:(\d+)$'

    tcp_match = re.match(tcp_pattern, target)
    tcp_ipv6_match = re.match(tcp_ipv6_pattern, target)

    if tcp_match:
        # IPv4:port format
        host = tcp_match.group(1)
        port = int(tcp_match.group(2))
        tcp_timeout = timeout if timeout is not None else 0.25
        return TcpTransport(host, port, tcp_timeout)

    elif tcp_ipv6_match:
        # [IPv6]:port format
        host = tcp_ipv6_match.group(1)
        port = int(tcp_ipv6_match.group(2))
        tcp_timeout = timeout if timeout is not None else 0.25
        return TcpTransport(host, port, tcp_timeout)

    else:
        # Assume serial port
        serial_timeout = timeout if timeout is not None else 0.1
        return SerialTransport(target, timeout=serial_timeout)


class Csv1Device:
    STATUS_OK = 0x0000

    def __init__(self, target, timeout=None):
        self.transport = create_transport(target, timeout)
        print(f"Connected via {self.transport.transport_type()} (timeout={self.transport.timeout}s)")

    def __del__(self):
        if hasattr(self, 'transport'):
            self.transport.close()

    # --- low level command builders ------------------------------------------
    def _cmd_direct(self, channel, value):
        return struct.pack(">BBH", channel, 0x00, value)

    def _cmd_bind_table(self, channel, table):
        return struct.pack(">BBH", channel, 0x10 + (table & 0x03), 0x0000)

    def _cmd_write_table(self, table, offset, value):
        return struct.pack(">BBH", 0x10 + (table & 0x03), offset, value)

    def _cmd_set_offset(self, offset):
        return struct.pack(">BBH", 0xFF, offset, 0x0000)

    def _cmd_gpio(self, channel, state):
        return struct.pack(">BBH", 0xFE, channel, 0x0001 if state else 0x0000)

    def _cmd_keepalive(self):
        return struct.pack(">BBH", 0xFD, 0x00, 0x0000)

    def _cmd_ldac(self):
        return struct.pack(">BBH", 0xFC, 0x00, 0x0000)

    def _cmd_register(self, register, value):
        return struct.pack(">BBH", 0xFB, register, value)

    # --- high level helpers ---------------------------------------------------
    def write_dac(self, channel, value16_be):
        self._send([self._cmd_direct(channel, value16_be)])

    def bind_table(self, channel, table):
        self._send([self._cmd_bind_table(channel, table)])

    def write_table(self, table, offset, values_be):
        cmds = []
        for i, v in enumerate(values_be):
            cmds.append(self._cmd_write_table(table, offset + i, v))
        self._send(cmds)

    def set_offset(self, offset):
        self._send([self._cmd_set_offset(offset)])

    def set_gpio(self, channel, state):
        self._send([self._cmd_gpio(channel, state)])

    def ldac(self):
        self._send([self._cmd_ldac()])

    def keepalive(self):
        self._send([self._cmd_keepalive()])

    def write_register(self, reg, value):
        self._send([self._cmd_register(reg, value)])

    def cfg_rs422(self, speed):
        self._send([
            self._cmd_register(1, speed >> 16),
            self._cmd_register(2, speed & 0xffff)
        ])

    def _send(self, commands):
        pkt = b"".join(commands)
        try:
            self.transport.write(pkt)
            rsp = self.transport.read(len(commands) * 2)

            if len(rsp) != len(commands) * 2:
                if len(rsp) == 0:
                    raise IOError(f"Timeout waiting for response (expected {len(commands) * 2} bytes, got 0)")
                else:
                    raise IOError(f"Partial response (expected {len(commands) * 2} bytes, got {len(rsp)})")

            for i in range(0, len(rsp), 2):
                status = struct.unpack(">H", rsp[i:i+2])[0]
                if status != self.STATUS_OK:
                    raise IOError("Device returned status 0x%04X" % status)

        except (socket.timeout, serial.SerialTimeoutException) as e:
            raise IOError(f"Communication timeout: {e}")
        except Exception as e:
            raise IOError(f"Communication error: {e}")

    def close(self):
        """Explicitly close the transport connection"""
        self.transport.close()


# -----------------------------------------------------------------------------
# Demo / Test routine
# -----------------------------------------------------------------------------
def main():
    parser = argparse.ArgumentParser(
        description="CSv1-OL8-IRS422 test program with serial and TCP support",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  Serial communication:
    %(prog)s /dev/ttyACM0
    %(prog)s COM5 --timeout 0.2

  TCP communication:
    %(prog)s 192.168.1.100:8080
    %(prog)s 192.168.56.102:2012 --timeout 0.5
    %(prog)s [::1]:8080 --timeout 0.1

  For interactive control, also try the Rust TUI tool:
    cargo run --bin tui_diagnostic -- 192.168.56.102:2012
        """
    )

    parser.add_argument(
        "target",
        help="Connection target: serial port (e.g. /dev/ttyACM0, COM5) or TCP address (e.g. 192.168.1.100:8080, [::1]:8080)"
    )
    parser.add_argument(
        "--timeout",
        type=float,
        help="Communication timeout in seconds (default: 0.1 for serial, 0.25 for TCP)"
    )
    parser.add_argument(
        "--verbose", "-v",
        action="store_true",
        help="Enable verbose output"
    )

    args = parser.parse_args()

    try:
        device = Csv1Device(args.target, args.timeout)
    except Exception as e:
        print(f"[ERROR] Failed to connect: {e}")
        sys.exit(1)

    # Ctrl-C handler
    def sigint_handler(signum, frame):
        print("\n[INFO] Ctrl-C detected – disabling GPIO1 and exiting...")
        try:
            device.set_gpio(1, False)
        except Exception as e:
            if args.verbose:
                print(f"[WARNING] Failed to disable GPIO1: {e}")
        finally:
            device.close()
        sys.exit(0)

    signal.signal(signal.SIGINT, sigint_handler)

    try:
        # Initialize all DAC channels to 0
        print("[INFO] Initializing DAC channels to 0...")
        for ch in range(8):
            device.write_dac(ch, 0)
            if args.verbose:
                print(f"  DAC {ch} = 0")

        print("[INFO] Enabling GPIO0 and GPIO1...")
        device.set_gpio(0, True)
        device.set_gpio(1, True)

        print("[INFO] Writing direct DAC values to channels 0-7...")
        direct_values = [0xffff, 0x0100, 0x1000, 0x8000, 0xFFFF]

        for i in range(5):
            for ch in range(8):
                value = direct_values[i]
                device.write_dac(ch, value)
                print(f"Output {ch} value 0x{value:04x} ({value})")
            time.sleep(1)

        print("[INFO] Binding channels to tables...")
        print("  Channels 0,1 -> table 0")
        print("  Channels 2,3 -> table 1")
        print("  Channels 4,5 -> table 0")
        print("  Channels 6,7 -> table 1")

        for ch in (0, 1):
            device.bind_table(ch, 0)
        for ch in (2, 3):
            device.bind_table(ch, 1)
        for ch in (4, 5):
            device.bind_table(ch, 0)
        for ch in (6, 7):
            device.bind_table(ch, 1)

        print("[INFO] Filling tables 0 (ascending) and 1 (descending) with demo data...")
        step = 16383  # 0x3FFF
        vals_inc = []
        vals_dec = []
        v_up = 0
        v_down = 0xFFFF

        for _ in range(10):
            vals_inc.append(v_up & 0xFFFF)
            vals_dec.append(v_down & 0xFFFF)
            v_up = (v_up + step) & 0xFFFF
            v_down = (v_down - step) & 0xFFFF

        if args.verbose:
            print("vals_inc:", [f"0x{v:04x}" for v in vals_inc])
            print("vals_dec:", [f"0x{v:04x}" for v in vals_dec])

        offset_base = 48  # ASCII '0'
        device.write_table(0, offset_base, vals_inc)
        device.write_table(1, offset_base, vals_dec)

        print(f"[INFO] Starting offset switching (20 cycles, 1s interval)...")
        for cycle in range(20):
            print(f"[INFO] Cycle {cycle+1}/20")
            for o in range(10):
                offset = offset_base + o
                print(f"  --> set offset = {offset} ('{chr(offset)}')")
                device.set_offset(offset)
                time.sleep(1)

        print("[INFO] Configuring RS422 to 2MHz...")
        device.cfg_rs422(2000000)

        print("[INFO] Entering KeepAlive mode (5s interval). Press Ctrl-C to exit.")
        keepalive_count = 0
        while True:
            device.keepalive()
            keepalive_count += 1
            print(f"[INFO] Keepalive {keepalive_count} sent")
            time.sleep(5.0)

    except KeyboardInterrupt:
        sigint_handler(signal.SIGINT, None)
    except Exception as e:
        print(f"[ERROR] Test failed: {e}")
        if args.verbose:
            import traceback
            traceback.print_exc()
        try:
            device.close()
        except:
            pass
        sys.exit(1)


if __name__ == "__main__":
    main()
