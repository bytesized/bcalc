# bcalc
A command line calculator

## Features

### Arbitrarily Large/Precise Number Support

In addition to allowing numbers to be arbitrarily large, bcalc stores non-integers via ratios rather than as floating point binary numbers. This means that precision isn't lost when binary floating point representations can't accurately represent a value. See [this Wikipedia article](https://en.wikipedia.org/wiki/Binary_number#Fractions) for more information on this problem.

Note that this approach can't really be used for irrational numbers. This isn't a problem for currently implemented features, but planned future features will be able to result in irrational numbers such as the square root of 2. The current plan is to use some sort of approximation in the case of irrational numbers. 

### Input History

Supports history backscroll via up and down arrow keys.

### Variables

Variable assignment supported through this syntax:

```
$var = 123
```

Variables can then be used in the place of numbers in later expressions.

### Multisession support

bcalc can remember the input and variable history from previous sessions. This feature currently won't work properly, however, unless the environment is set up properly. This set up is performed automatically when via the installer for my [utilities](https://github.com/bytesized/utilities).

### Commands

bcalc has support for several commands which are invoked by beginning the calculator input with a `/`. More information on commands is available via the `help` command. Without arguments, it lists the available commands. If it is given a command name as an argument, it gives more detailed information about that command. For example:

```
/help
/help help
```

### Hotkeys

bcalc supports several navigation hotkeys:

 - Larger movement distance with arrow keys by additionally using Control
 - Control+M when the cursor is over a parenthesis to jump to the matching one.

## TODO

This project is still a work in progress. A number of features are planned or do not yet work properly:

 - Fractional exponents
 - The square root operator `sqrt`
