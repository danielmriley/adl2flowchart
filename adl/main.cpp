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
  std::string fileName = argv[argc - 1];
  std::ifstream fin(fileName);
  adl::Driver drv(&fin);
  // exit(0);
  int res = drv.parse();


  if(res == 0) std::cout << "Parsing successful!\n";

  if(res == 0) std::cout << "ast.size(): " << drv.ast.size() << "\n";
  if(res == 0) { res = drv.setTables(); }
  else std::cerr << "Failed Parsing()\n";

  if(res == 0) { res = adl::checkDecl(drv); }
  else std::cerr << "Failed setTables()\n";

  if(res == 0) { res = adl::typeCheck(drv); }
  else std::cerr << "Failed checkDecl()\n";

  if(res == 0) { res = drv.visitAST(adl::printAST); } // run "dot -Tpdf ast.dot -o ast.pdf" to create a PDF
  else std::cerr << "Failed typeCheck()\n";

  if(res == 0) { res = adl::printFlowChart(drv); } // run "dot -Tpdf fc.dot -o fc.pdf" to create a PDF
  else std::cerr << "Failed printAST()\n";

  if(res == 0) {
    res = drv.ast2cuts(&adl::parts,&adl::NodeVars,&adl::ListParts,&adl::NodeCuts,
                 &adl::BinCuts, &adl::ObjectCuts,
                 &adl::NameInitializations, &adl::TRGValues,
                 &adl::ListTables, &adl::cntHistos, &adl::systmap);
  }
  std::cout << "\n\nPART: ";
  for(auto& l: adl::ListParts) {
    std::cout << l.first << ", ";
    if(l.second[0] == nullptr) std::cout << "NULLPTR";
  }
  std::cout << "\n\nOBJ: ";
  for(auto& l: adl::ObjectCuts) {
    std::cout << l.first << ", ";
    if(l.second == nullptr) std::cout << "NULLPTR";
  }
  std::cout << "\n\nNODE: ";
  for(auto& l: adl::NodeVars) {
    std::cout << l.first << ", ";
    if(l.second == nullptr) std::cout << "NULLPTR";
  }
  std::cout << "\n\nCUTS: ";
  for(auto& l: adl::NodeCuts) {
    std::cout << l.first << ", ";
    if(l.second == nullptr) std::cout << "NULLPTR";
  }
  std::cout << "\nParts: " << adl::parts.size() << "\n";
  std::cout << "NodeVars: " << adl::NodeVars.size() << "\n";
  std::cout << "ListParts: " << adl::ListParts.size() << "\n";
  std::cout << "NodeCuts: " << adl::NodeCuts.size() << "\n";
  std::cout << "BinCuts: " << adl::BinCuts.size() << "\n";
  std::cout << "ObjectCuts: " << adl::ObjectCuts.size() << "\n";
  std::cout << "NameInitializations: " << adl::NameInitializations.size() << "\n";
  std::cout << "TRGValues: " << adl::TRGValues.size() << "\n";
  std::cout << "ListTables: " << adl::ListTables.size() << "\n";
  std::cout << "cntHistos: " << adl::cntHistos.size() << "\n";
  std::cout << "systmap: " << adl::systmap.size() << "\n";

  std::cout << "\n";
  // if(res == 0) for(auto d: drv.objectTable) std::cout << "o: " << d << "\n";
  // if(res == 0) for(auto d: drv.definitionTable) std::cout << "d: " << d << "\n";
  // if(res == 0) for(auto d: drv.regionTable) std::cout << "r: " << d << "\n";
  if(res == 0) std::cout << "finished\n";
  else std::cout << "ERROR\n";
  return res;
}
