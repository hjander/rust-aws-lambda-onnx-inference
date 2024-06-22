#!/bin/bash

# Retrieve the API URL
API_URL=http://localhost:9000
echo ${API_URL}
curl -H "Accept: image/jpeg" -X POST ${API_URL} -H "Content-Type: image/jpeg" --data-binary @./data/baseball.jpg -o annotated.jpeg