#!/bin/bash

docker build -t matchbox_server .; docker run -p 3536:3536 matchbox_server