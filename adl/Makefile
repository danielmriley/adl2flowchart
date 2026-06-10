CC=clang++
YACC=bison
LEX=flex

LFILE=scanner.l
YFILE=parser.y
LCPP=Scanner.cpp
YCPP=Parser.cpp
YHPP=Parser.h
LOCHH=location.hh

SRCS=main.cpp driver.cpp $(LCPP) $(YCPP) external_functions.cpp \
     semantic_checks.cpp region_analysis.cpp constraint_encoder.cpp \
     cutlang_declares.cpp
HDRS=ast.hpp driver.h semantic_checks.h region_analysis.hpp \
     region_formula.h constraint_encoder.hpp cutlang_declares.h scanner.hpp

CXXFLAGS=-g -std=c++17 -Wall

EXE=smash

all: $(EXE)

$(EXE): $(SRCS) $(HDRS)
	@$(CC) $(CXXFLAGS) $(filter %.cpp,$(SRCS)) -o $(EXE)

$(YCPP): $(YFILE)
	@$(YACC) $(YFILE)

$(LCPP): $(LFILE)
	@$(LEX) $(LFILE)


clean:
	@rm -f $(LCPP) $(YCPP) $(YHPP) ../$(EXE) $(LOCHH) $(EXE)
