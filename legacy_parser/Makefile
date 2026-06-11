# Makefile of adl root dir

all:
	@make -s -C ./adl/
	@cp ./adl/smash ./

test: all
	@./scripts/run_golden_tests.sh
	@./scripts/validate_corpus.sh
	@./scripts/phase2_z3_spike.sh

test-disjoint: all
	@./scripts/run_golden_tests.sh

test-corpus: all
	@./scripts/validate_corpus.sh

test-z3-spike: all
	@./scripts/phase2_z3_spike.sh

clean:
	@make -C ./adl/ -f Makefile clean
