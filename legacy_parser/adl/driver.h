#ifndef DRIVER_H
#define DRIVER_H

#include "scanner.hpp"
#include "Parser.h"
#include "ast.hpp"

#include <map>
#include <set>
#include <list>
#include <utility>
#include <cstdio>
#include <fstream>
#include <algorithm>
#include <filesystem>

namespace fs = std::filesystem;

namespace adl {

  // Particle families for the typeTable / object disjointness analysis.
  enum particleType {
    none_t = 0,
    electron_t = 1,
    jet_t = 2,
    bjet_t = 3,
    lightjet_t = 4,
    muonlikeV_t = 5,
    electronlikeV_t = 6,
    pureV_t = 7,
    photon_t = 8,
    fjet_t = 9,
    truth_t = 10,
    tau_t = 11,
    muon_t = 12,
    track_t = 19,
    combo_t = 20,
    consti_t = 21
  };

  class Driver
  {
  public:

    friend class Parser;
    friend class Scanner;

    Driver(std::istream *in);

    int parse();
    int parse(std::string);
    int visitAST(int (*f)(ExprVector& ast));
    void fillTypeTable();
    void setDependencyChart();
    int check_function_table(std::string id);
    int check_object_table(std::string id);
    int check_property_table(std::string id);

    void loadFromLibraries();
    std::string getBinType(Expr* expr);
    int setTables();
    void addNode(Expr*);
    int addObject(std::string id,std::string takeType);
    int addRegion(std::string id);
    int addDefine(std::string id);
    std::string getObjectDeclType(std::string s);
    std::string getVarNodeType(std::string vn);

    int checkObjectTable(std::string id);
    int checkDefinitionTable(std::string id);
    int checkRegionTable(std::string id);

    std::vector<Expr*> ast;
    // map of object name to either PARENT (predefined) or TAKE type (declared)
    std::map<std::string,std::string> objectTable; // objectTable[NAME] = TYPE
    std::vector<std::string> regionTable;
    std::vector<std::string> definitionTable;
    std::map<std::string, int> typeTable;
    std::map<std::string, std::vector<std::string>> dependencyChart;

    std::string findDep(std::string var);
    void processDefBinNode(DefineNode* dn, Expr* body);
    fs::path getLibPath() const { return libPath; }

  private:
    Scanner scanner;
    Parser parser;
    unsigned int loc;
    unsigned int location();

    fs::path libPath;
    std::string objsLib = "ext_objs.txt";
    std::string functionsLib = "ext_lib.txt";
    std::string propertiesLib = "property_vars.txt";

    void incrementLocation(unsigned int loc);
  }; // end driver class
} // end adl namespace

#include "semantic_checks.h"

#endif
