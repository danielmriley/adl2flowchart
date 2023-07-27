#include <iostream>
#include <sstream>
#include <string>
#include <map>

// #include "cutlang_declares.h"
#include "scanner.hpp"
#include "Parser.h"
#include "driver.h"

std::map<std::string,std::string> function_map;

int set_function_map() {
  std::ifstream fin("adl/property_vars.txt");
  if(!fin.good()) {
    std::cerr <<"FAILED TO CONNECT TO FILE\n";
    fin.close();
    exit(1);
  }

  std::string input;
  while(std::getline(fin, input)) {
    std::stringstream ss(input);
    std::string property, arrow, func_name;
    ss >> property; ss >> arrow; ss >> func_name;

    function_map[property] = func_name;
  }

  fin.close();
  return 0;
}

int main(int argc, char **argv) {
  set_function_map();
  // for(auto& m: function_map) std::cout << m.first << " -> " << m.second << "\n";
  //exit(0);
  adl::Driver drv;
  std::string fileName = argv[argc - 1];
  int res = drv.parse(fileName);


  if(res == 0) std::cout << "Parsing successful!\n";

  if(res == 0) std::cout << "ast.size(): " << drv.ast.size() << "\n";
  if(res == 0) { drv.setTables(); }
  else std::cerr << "Failed Parsing()\n";

  if(res == 0) { adl::checkDecl(drv); }
  else std::cerr << "Failed setTables()\n";

  if(res == 0) { adl::typeCheck(drv); }
  else std::cerr << "Failed checkDecl()\n";

  if(res == 0) { drv.visitAST(adl::printAST); } // run "dot -Tpdf ast.dot -o ast.pdf" to create a PDF
  else std::cerr << "Failed typeCheck()\n";

  if(res == 0) { adl::printFlowChart(drv); } // run "dot -Tpdf fc.dot -o fc.pdf" to create a PDF
  else std::cerr << "Failed printAST()\n";

  if(res == 0) {
    drv.ast2cuts(&adl::parts,&adl::NodeVars,&adl::ListParts,&adl::NodeCuts,
                 &adl::BinCuts, &adl::ObjectCuts,
                 &adl::NameInitializations, &adl::TRGValues,
                 &adl::ListTables, &adl::cntHistos, &adl::systmap);
  }
  // if(res == 0) for(auto d: drv.objectTable) std::cout << "o: " << d << "\n";
  // if(res == 0) for(auto d: drv.definitionTable) std::cout << "d: " << d << "\n";
  // if(res == 0) for(auto d: drv.regionTable) std::cout << "r: " << d << "\n";
  if(res == 0) std::cout << "finished\n";
  else std::cout << "ERROR\n";
  return res;
}
