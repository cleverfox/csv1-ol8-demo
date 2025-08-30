#!/usr/bin/env python3
import serial
import struct
import time
import argparse
import signal
import sys


class Csv1Device(serial.Serial):
    STATUS_OK = 0x0000

    def __init__(self, port):
        super().__init__(port=port, baudrate=115200, timeout=0.1)

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
        self._send([self._cmd_register(reg,value)])

    def cfg_rs422(self, speed):
        self._send([
            self._cmd_register(1, speed >> 16),
            self._cmd_register(2, speed & 0xffff)
            ])

    def _send(self, commands):
        pkt = b"".join(commands)
        self.write(pkt)
        rsp = self.read(len(commands) * 2)
        if len(rsp) != len(commands) * 2:
            raise IOError("Timeout waiting for response")
        for i in range(0, len(rsp), 2):
            status = struct.unpack(">H", rsp[i:i+2])[0]
            if status != self.STATUS_OK:
                raise IOError("Device returned status 0x%04X" % status)

# -----------------------------------------------------------------------------
# Demo / Test routine
# -----------------------------------------------------------------------------
def main():
    parser = argparse.ArgumentParser(description="CSv1-OL8-IRS422 test program")
    parser.add_argument("port", help="Serial port (e.g. /dev/ttyACM0 or COM5)")
    args = parser.parse_args()

    device = Csv1Device(args.port)

    # Ctrl-C handler
    def sigint_handler(signum, frame):
        print("\n[INFO] Ctrl-C detected – disabling GPIO1 and exiting...")
        try:
            device.set_gpio(1, False)
        except Exception:
            pass
        sys.exit(0)

    signal.signal(signal.SIGINT, sigint_handler)

    for ch in range(8):
        device.write_dac(ch, 0)

    print("[INFO] Enabling GPIO0 and GPIO1 ...")
    device.set_gpio(0, True)
    device.set_gpio(1, True)


    print("[INFO] Writing direct DAC values to channels 0..3 ...")
    direct_values = [0xffff, 0x0100, 0x1000, 0x8000, 0xFFFF]
#    for ch in range(8):
#        print(f"Output {ch} value 0xffff")
#        device.write_dac(ch, 0xffff)
#        input()
#        print(f"Output {ch} value 0x0")
#        device.write_dac(ch, 0)

    for i in range(5):
        for ch in range(8):
            print(f"Output {ch} value {direct_values[i]}")
            device.write_dac(ch, direct_values[i])
        time.sleep(1)

    print("[INFO] Binding channels 4-5 to table0 and 6-7 to table1 ...")
    for ch in (0, 1):
        device.bind_table(ch, 0)
    for ch in (2, 3):
        device.bind_table(ch, 1)
    for ch in (4, 5):
        device.bind_table(ch, 0)
    for ch in (6, 7):
        device.bind_table(ch, 1)

    print("[INFO] Filling tables 0 (ascending) and 1 (descending) with demo data ...")
    step = 16383; #0x7281
    vals_inc = []
    vals_dec = []
    v_up = 0
    v_down = 0xFFFF
    for _ in range(10):
        vals_inc.append(v_up & 0xFFFF)
        vals_dec.append(v_down & 0xFFFF)
        v_up = (v_up + step) & 0xFFFF
        v_down = (v_down - step) & 0xFFFF

    print("vals_inc", vals_inc)
    offset_base = 48  # ASCII '0'
    device.write_table(0, offset_base, vals_inc)
    device.write_table(1, offset_base, vals_dec)

    print("[INFO] Starting offset switching (2 cycles, 1s interval) ...")
    for cycle in range(20):
        print(f"[INFO] Cycle {cycle+1}/2")
        for o in range(10):
            print(f"  --> set offset = {offset_base + o}")
            device.set_offset(offset_base + o)
            #device.ldac()
            time.sleep(1)

    device.cfg_rs422(2000000)
    print("[INFO] Entering KeepAlive mode (5s interval). Press Ctrl-C to exit.")
    while True:
        device.keepalive()
        time.sleep(5.0)


if __name__ == "__main__":
    main()
