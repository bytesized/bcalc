# bcalc
A command line calculator

## Features

### Arbitrarily Large/Precise Number Support

In addition to allowing numbers to be arbitrarily large, bcalc stores non-integers via ratios rather than as floating point binary numbers. This means that precision isn't lost when binary floating point representations can't accurately represent a value. See [this Wikipedia article](https://en.wikipedia.org/wiki/Binary_number#Fractions) for more information on this problem.

Note that this approach can't really be used for irrational numbers. Operations that result in irrational numbers such as `sqrt 2` will use the configurable precision values to determine how many digits of precision to calculate. See `/help precision` for more details.

### Input History

Supports history backscroll via up and down arrow keys.

### Variables

Variable assignment supported through this syntax:

```
$var = 123
```

Variables can then be used in the place of numbers in later expressions.

### Multisession support

bcalc can remember the input and variable history from previous sessions. This feature currently won't work properly, however, unless the environment is set up properly. This set up is performed automatically when installed via [my utilities](https://github.com/bytesized/utilities) installer.

### Commands

bcalc has support for several commands which are invoked by beginning the calculator input with a `/`. More information on commands is available via the `help` command. Without arguments, it lists the available commands. If it is given a command name as an argument, it gives more detailed information about that command. For example:

```
/help
/help help
```

### Consistent exit key

Control+D exits on all operating system including when using `-a`.

### Hotkeys

bcalc supports several navigation hotkeys:

 - Larger movement distance with arrow keys by additionally using Control or Shift.
 - Control+N when the cursor is over a parenthesis to jump to the matching one.

## TODO

This project is still a work in progress. A number of features are planned or do not yet work properly:

 - Allow argument configuration values to be saved.
 - Enable more detailed errors that point at the location of the error in the input.
 - Add a `/quit` command.
 - Add logarithm support.
 - Add common constants such as pi.
 - Add trigonometric functions.
 - Support for imaginary numbers.
 - Add a way of cancelling long calculations.
