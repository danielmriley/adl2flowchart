# Makefile of adl root dir

all:
	make -s -C ./adl/
	mv ./adl/smash ./

clean:
	make -C ./adl/ -f Makefile clean