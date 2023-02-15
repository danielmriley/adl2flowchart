#ifndef DRIVER_HH
#define DRIVER_HH
#include "scanner.hpp"
#include "Parser.h"
#include "ast.hpp"
#include <map>
#include <cstdio>

namespace adl {
  class Driver
  {
  public:

    friend class Parser;
    friend class Scanner;

    Driver();

    int parse();
    int visitAST(int (*f)(ExprVector& ast));

    int setTables();
    void addNode(Expr*);
    void addObject(std::string id);
    void addRegion(std::string id);
    void addDefine(std::string id);

    int checkObjectTable(std::string id);

    std::vector<Expr*> ast;
    std::vector<std::string> objectTable;
    std::vector<std::string> regionTable;
    std::vector<std::string> regionVarsTable;
    std::vector<std::string> definitionTable;

  private:
    Scanner scanner;
    Parser parser;
    unsigned int loc;
    unsigned int location();

    void incrementLocation(unsigned int loc);
  }; // end driver class
} // end adl namespace

#endif
