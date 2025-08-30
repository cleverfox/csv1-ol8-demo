# CSv1-OL8-IRS422 — Example Python Library and Test Program

This repository contains:

- `CSv1-OL8-IRS422.py` – Python library with class `Csv1Device` and a built-in demo / test sequence
- `requirements.txt` – list of dependencies (`pyserial`)

## Installation

```bash
pip install -r requirements.txt
````

## Usage

```bash
python CSv1-OL8-IRS422.py <serial_port>
```

Example:

```bash
python CSv1-OL8-IRS422.py /dev/ttyACM0
```

or (for Windows):

```bash
python CSv1-OL8-IRS422.py COM5
```

## Test sequence (demo)

The demo performs the following actions:

| Step | Description                                                                       |
| ---- | --------------------------------------------------------------------------------- |
| 1    | Write fixed values to DAC channels 0..3                                           |
| 2    | Bind channels 4..5 to table 0 and 6..7 to table 1                                 |
| 3    | Fill table 0 (ascending) and table 1 (descending) with 10 values (offsets 48..57) |
| 4    | Enable GPIO0 and GPIO1                                                            |
| 5    | Perform two rotation cycles of table offset, one offset per second                |
| 6    | Enter keep-alive mode (every 5 s) until Ctrl-C                                    |

At program exit (Ctrl-C), GPIO1 is switched off. GPIO0 will automatically switch off after keepalive timeout.

