# Root delegator. Legacy tool lives in legacy_parser/; the ADL2
# reimplementation spec (and eventually the adl2/ workspace) lives in
# reimplementation/.

all:
	@$(MAKE) -s -C legacy_parser

test:
	@$(MAKE) -s -C legacy_parser test

clean:
	@$(MAKE) -s -C legacy_parser clean
