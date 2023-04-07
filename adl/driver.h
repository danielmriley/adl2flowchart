#ifndef DRIVER_HH
#define DRIVER_HH

#include "scanner.hpp"
#include "Parser.h"
#include "ast.hpp"
#include "cutlang_declares.h"

#include <map>
#include <list>
#include <utility>
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
    int ast2cuts(std::list<std::string> *parts,std::map<std::string,Node*>* NodeVars,
                 std::map<std::string, std::vector<myParticle*> >* ListParts,
                 std::map<int,Node*>* NodeCuts,
                 std::map<int,Node*>* BinCuts, std::map<std::string,Node*>* ObjectCuts,
                 std::vector<std::string>* Initializations,
                 std::vector<int>* TRGValues, std::map<std::string,
                 std::pair<std::vector<float>, bool> >* ListTables,
                 std::map<std::string, std::vector<cntHisto> >*cntHistos,
                 std::map<int, std::vector<std::string> > *systmap);

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
