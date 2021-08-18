#!/bin/bash

pushd webapp
	npm run build
popd
rm -rf http/web
cp -r webapp/build http/web

