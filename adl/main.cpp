#include <iostream>
#include "cutlang_declares.h"
#include "scanner.hpp"
#include "Parser.h"
#include "driver.h"
#include <sstream>
#include <string>
#include <map>

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
  for(auto& m: function_map) std::cout << m.first << " -> " << m.second << "\n";
  exit(0);
  adl::Driver drv;
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

// Make sure the selection of muonsVeto has at least one element. line 148.
// Check for cyclic dependencies
//
















// What can we do model checking on?
// Type checking, type inference.
// Dig down into the system more to find where loops are made. There is ONE main loop through all of the events they want.
// Where is the c++ file written? A histogram or a Root file is produced.
// Where do they compile the c++? Not compiled
//
// Look at dependencies of source file and the data and catch ASAP in execution.
