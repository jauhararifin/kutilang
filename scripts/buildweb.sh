#!/bin/bash

pushd webapp
  npm install
  npm run build
popd
rm -rf http/web
cp -r webapp/build http/web

