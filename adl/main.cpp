#include <iostream>
#include "scanner.hpp"
#include "Parser.h"
#include "driver.h"
#include "semantic_checks.h"

int main(int argc, char **argv) {
    adl::Driver drv;
    int res = drv.parse();

    if(res == 0) std::cout << "Parsing successful!\n";
    else std::cout << "Parsing failed.\n";

    if(res == 0) std::cout << "ast.size(): " << drv.ast.size() << "\n";
    if(res == 0) { drv.setTables(); }
    if(res == 0) { adl::checkDecl(drv); }
    if(res == 0) { drv.visitAST(adl::typeCheck); }
    if(res == 0) { drv.visitAST(adl::printAST); } // run "dot -Tpdf ast.dot -o ast.pdf" to create a PDF

    if(res == 0) for(auto d: drv.objectTable) std::cout << "o: " << d << "\n";
    if(res == 0) for(auto d: drv.definitionTable) std::cout << "d: " << d << "\n";
    if(res == 0) for(auto d: drv.regionTable) std::cout << "r: " << d << "\n";

    return res;
}


// What can we do model checking on?
// Type checking, type inference.
// Dig down into the system more to find where loops are made. There is ONE main loop through all of the events they want.
// Where is the c++ file written? A histogram or a Root file is produced.
// Where do they compile the c++? Not compiled
//
// Look at dependencies of source file and the data and catch ASAP in execution.
