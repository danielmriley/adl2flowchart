#ifndef DRIVER_HH
#define DRIVER_HH

#include "scanner.hpp"
#include "Parser.h"
#include "ast.hpp"
#include "cutlang_declares.h"
#include "semantic_checks.h"

#include <map>
#include <set>
#include <list>
#include <utility>
#include <cstdio>
#include <fstream>

namespace adl {

  class Driver
  {
  public:

    friend class Parser;
    friend class Scanner;

    Driver();

    int parse();
    int parse(std::string);
    int visitAST(int (*f)(ExprVector& ast));

    void loadFromLibraries();
    std::string getBinType(Expr* expr);
    int setTables();
    void addNode(Expr*);
    int addObject(std::string id,std::string takeType);
    int addRegion(std::string id);
    int addDefine(std::string id);
    std::string getObjectDeclType(std::string s);

    int checkObjectTable(std::string id);
    int checkDefinitionTable(std::string id);
    int checkRegionTable(std::string id);
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
    // map of object name to either PARENT (predefined) or TAKE type (declared)
    std::map<std::string,std::string> objectTable;
    std::vector<std::string> regionTable;
//    std::vector<std::string> regionVarsTable;
    std::vector<std::string> definitionTable;

    // std::map<std::string,PropFunction> function_map;
    // std::map<std::string,LFunction> lfunction_map;
    // std::map<std::string,UnFunction> unfunction_map;
    // std::map<std::string,pair<particleType,std::string>> particle_map;

  private:
    Scanner scanner;
    Parser parser;
    unsigned int loc;
    unsigned int location();

    void incrementLocation(unsigned int loc);
  }; // end driver class
} // end adl namespace

#endif
