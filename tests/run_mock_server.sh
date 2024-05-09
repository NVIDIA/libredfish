#!/bin/bash

pip install --requirement requirements.txt

PYTHON=$(which python)
if [ "$PYTHON" = "" ]; then
	PYTHON=python3
fi
$PYTHON redfishMockupServer.py --port 1266 --dir mockups/dell/ --ssl --cert cert.pem --key key.pem

