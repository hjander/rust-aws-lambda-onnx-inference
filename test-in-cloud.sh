#!/bin/bash

# Retrieve the API URL
API_URL=$(aws cloudformation describe-stacks --stack-name rust-lambda-inference --query "Stacks[0].Outputs[?OutputKey=='InferenceApi'].OutputValue" --output text)
echo ${API_URL}
curl -H "Accept: image/jpeg" -X POST ${API_URL} -H "Content-Type: image/jpeg" --data-binary @./data/dog.jpg -o annotated.jpeg