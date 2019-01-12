# Plato reader - Remarkable Tablet port fork

![Logo](artworks/plato-logo.svg)

This is a fork of the [Plato document reader](https://github.com/baskerville/plato) for the reMarkable tablet.  It has been modified to use [libremarkable](https://github.com/canselcik/libremarkable) for rendering.

## Installation

Download a release zip from [Releases](http://github.com/darvin/plato/releases) and copy it to the reMarkable.

``` bash
mkdir /home/root/plato
mv plato_release.zip /home/root/plato
cd /home/root/plato
unzip plato_release.zip
./remarkable_install.sh
```

Stop xochitl with `systemctl stop xochitl`.

Start Plato with `systemctl start plato`.

## Screenshots

[![Tn01](artworks/thumbnail01.png)](artworks/screenshot01.png) [![Tn02](artworks/thumbnail02.png)](artworks/screenshot02.png)

[![Donate](https://img.shields.io/badge/Donate-PayPal-green.svg)](https://www.paypal.com/cgi-bin/webscr?cmd=_s-xclick&hosted_button_id=KNAR2VKYRYUV6)
