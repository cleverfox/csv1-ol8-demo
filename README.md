# USB Serial Device Controller (libusb implementation)

This is a Rust application that communicates with a custom USB device using libusb directly, bypassing the CDC (Communication Device Class) driver. This implementation was created to work around buggy USB CDC implementations that don't work properly on FreeBSD.

## Device Information

- **Vendor ID**: 0xdead
- **Product ID**: 0xcafe
- **Manufacturer**: klong
- **Device**: csv1-ol8

## Features

- Direct USB communication using libusb (rusb crate)
- Automatic device detection by VID/PID
- Proper USB interface claiming and kernel driver detachment
- CDC payload padding to multiples of 4 bytes as required by the device
- DAC control and GPIO manipulation
- Table-based waveform generation

## Protocol

The device uses a 4-byte command protocol:

| First byte  | Second byte  | third & 4th bytes | Description |
|-------------|--------------|-------------------|-------------|
| n = 0..7    | 0x00         | vv                | DirectWrite DAC(n)=vv |
| n = 0..7    | i+16 (16..19)| vv                | AttachTable DAC(n)=Table(i) |
| i+16(16..19)| n (0..255)   | vv                | Table(i)[n]=vv |
| 0xff        | n (0..255)   | 0x0000            | UseTable |
| 0xfe        | n (0..7)     | 0x0000..0x0001    | control GPIOn |
| 0xfd        | 0x00         | 0x0000            | KeepAlive (to avoid disabling GPIO0) |
| 0xfc        | 0x00         | 0x0000            | LDAC - update DACs with loaded values |

## Building

```bash
cargo build --release
```

## Running

### Device Detection Utility

First, you can check if your device is detected:

```bash
cargo run --bin detect_device
```

This utility will:
- Scan all USB devices
- Show detailed information about your target device if found
- Help diagnose permission and driver issues

### Main Application

```bash
cargo run
```

Or run the compiled binary:

```bash
./target/release/serialtest
```

## Prerequisites

### FreeBSD
- libusb is typically available by default
- You may need to run as root or add your user to the `operator` group for USB device access
- Consider adding a devd rule for automatic permissions

### Linux
- libusb development packages may be needed: `sudo apt-get install libusb-1.0-0-dev` (Debian/Ubuntu)
- You may need udev rules for device permissions

### macOS
- libusb can be installed via Homebrew: `brew install libusb`
- No special permissions typically needed

## Permissions

On Unix-like systems, you may need appropriate permissions to access USB devices directly. Options include:

1. **Run as root** (not recommended for regular use)
2. **Add udev rules** (Linux) or **devd rules** (FreeBSD)
3. **Add user to appropriate group** (varies by system)

### Example udev rule (Linux)
Create `/etc/udev/rules.d/99-csv1-ol8.rules`:
```
SUBSYSTEM=="usb", ATTR{idVendor}=="dead", ATTR{idProduct}=="cafe", MODE="0666", GROUP="plugdev"
```

### Example devd rule (FreeBSD)
Add to `/etc/devd.conf` or create `/usr/local/etc/devd/csv1-ol8.conf`:
```
notify 100 {
    match "system" "USB";
    match "subsystem" "DEVICE";
    match "type" "ATTACH";
    match "vendor" "0xdead";
    match "product" "0xcafe";
    action "chmod 666 /dev/ugen*";
};
```

## Differences from CDC Version

1. **No serial port path needed** - Device is found automatically by VID/PID
2. **No baud rate configuration** - USB bulk transfers are used directly
3. **Automatic padding** - Commands are padded to 4-byte boundaries as required
4. **Better error handling** - Distinguishes between different types of USB errors
5. **Cross-platform compatibility** - Works on FreeBSD, Linux, macOS, and Windows

## Troubleshooting

### Device not found
- Run the device detection utility first: `cargo run --bin detect_device`
- Ensure the device is connected and powered
- Check that VID/PID match your device: `lsusb | grep dead` (Linux) or `usbconfig list` (FreeBSD)
- Verify permissions to access USB devices

### Permission denied
- Try running as root temporarily to test
- Set up appropriate udev/devd rules
- Check that no other driver has claimed the device

### Build failures
- Ensure libusb development libraries are installed
- On some systems, you may need to set `PKG_CONFIG_PATH`

## Hardware Connection

The application expects the device to be connected via USB and enumerated with the specified VID/PID. No additional drivers should be loaded for the device interface that will be used for communication.

## License

This project uses the same license terms as the original serialport version.