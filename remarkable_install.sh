#!/bin/bash
# Install plato systemd unit

# quit if we get any errors
set -e

# check for correct unarchive path
unarchived_dir=$(
    cd $(dirname "$0")
    pwd
)
if [[ ! $unarchived_dir = /home/root/plato ]]
then
    echo "Detected incorrect path.  Please unarchive tarball in /home/root/plato/"
    exit
fi

# create books/
if [[ ! -d $HOME/books ]]
then
    mkdir $HOME/books
fi

# install systemd unit
cp plato.service /etc/systemd/system
systemctl daemon-reload
