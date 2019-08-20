ls `pwd`/.run | xargs -n 1 -I CID_FILE -- bash -c 'docker rm -vf $(cat `pwd`/.run/CID_FILE)'
