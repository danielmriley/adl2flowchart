# Makefile of adl root dir

all:
	@make -s -C ./adl/
	@cp ./adl/smash ./

clean:
	@make -C ./adl/ -f Makefile clean
