#!/bin/sh
echo "Start a daemon using the debug build profile (not the release profile) and press any key to continue..."
read -r _

wget http://localhost:11987/api.json -O openapi.json

echo "Done. Don't forget to commit and copy the new spec to the website."
