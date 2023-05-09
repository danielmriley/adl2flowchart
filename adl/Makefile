CC=clang++
YACC=bison
LEX=flex

LFILE=scanner.l
YFILE=parser.y
LCPP=Scanner.cpp
YCPP=Parser.cpp
YHPP=Parser.h
MAIN=main.cpp
DH=driver.h
DCPP=driver.cpp
EXTFUN=external_functions.cpp
ASTHPP=ast.hpp
SEMCHH=semantic_checks.h
SEMCHCPP=semantic_checks.cpp
LOCHH=location.hh
CLDEFS=cutlang_declares.h

EXE=smash

all: $(EXE)

$(EXE): $(YCPP) $(LCPP) $(MAIN) $(DCPP) $(EXTFUN) $(ASTHPP) $(SEMCHH) $(SEMCHCPP) $(DH) $(CLDEFS)
	@$(CC) -g $(MAIN) $(DCPP) $(LCPP) $(YCPP) $(EXTFUN) $(SEMCHCPP) -o $(EXE)

$(YCPP): $(YFILE)
	@$(YACC) $(YFILE)

$(LCPP): $(LFILE)
	@$(LEX) $(LFILE)


clean:
	@rm $(LCPP) $(YCPP) $(YHPP) ../$(EXE) $(LOCHH) $(EXE)
