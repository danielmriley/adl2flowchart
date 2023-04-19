#!/bin/bash

NEWDIR=tests/results/$(date +%m-%d-%Y_%T)/
mkdir $NEWDIR

if [[ $2 == "--tutorials" ]]; then
  for FILE in $examples/tutorials/*; do
    echo $FILE
    OUTPUTFILE="${FILE##*/}"
    OUTPUTFILE="output_${OUTPUTFILE%.*}.txt"
    { ./smash; } < $FILE 2> $OUTPUTFILE > /dev/null
    mv $OUTPUTFILE $NEWDIR$OUTPUTFILE
  done
  exit()
fi

for DIR in examples/*; do
  for FILE in $DIR/*; do
    echo $FILE
    OUTPUTFILE="${FILE##*/}"
    OUTPUTFILE="output_${OUTPUTFILE%.*}.txt"
    { ./smash; } < $FILE 2> $OUTPUTFILE > /dev/null
    mv $OUTPUTFILE $NEWDIR$OUTPUTFILE
  done
done
