# Accurate

Measure accuracy of your mechanical watches in two steps.

1. Run the program with `--sync` argument. You need this step if your watch has not been synchronized yet or if it has been stopped or set. When the seconds hand reaches 12 o'clock click with your left mouse button. The program will save that timestamp, based on atomic clock time.

2. After at least 24 hours run the program again but without the `--sync` argument. When the seconds hand reaches 12 o'clock click with your mouse. The program will save that timestamp, based on atomic clock time. The program compares the drift between the measurement at the sync phase and the current measurement and calculates the watch daily accuracy. To measure again, repeat this step later. The accuracy measurement becomes more precise the longer the interval between sync and measurement.

### Usage

Usage: accurate [OPTIONS]

Options:
  -s, --sync               Synchronize your watch
  -n, --name <NAME>        Name of the watch to measure [default: main]
  -d, --data <DATA>        Database file [default: watch.sqlite]
  -c, --comment <COMMENT>  Comment of the measurement if any [default: ]
  -h, --help               Print help
  -V, --version            Print version
