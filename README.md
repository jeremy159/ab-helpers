## Building docker image

`docker build -t ab-helpers .`

## Launching docker container with newly generated image
1. First delete the old one from Docker Desktop
2. `docker run -d --restart unless-stopped -p 80:80 ab-helpers`