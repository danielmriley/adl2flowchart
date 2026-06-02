#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <map>

// #include "cutlang_declares.h"
#include "scanner.hpp"
#include "Parser.h"
#include "driver.h"
#include "region_analysis.hpp"
#include "semantic_checks.h"

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

  bool doRegionAnalysis = false;
  bool regionSmt = false;
  bool regionJsonStdout = false;
  std::string regionJsonPath;
  std::string fileName;

  for (int i = 1; i < argc; ++i) {
    std::string arg = argv[i];
    if (arg == "--region-analysis" || arg == "-r") {
      doRegionAnalysis = true;
    } else if (arg == "--smt") {
      doRegionAnalysis = true;
      regionSmt = true;
    } else if (arg == "--json") {
      doRegionAnalysis = true;
      if (i + 1 < argc && argv[i + 1][0] != '-') {
        regionJsonPath = argv[++i];
      } else {
        regionJsonStdout = true;
      }
    } else if (!arg.empty() && arg[0] != '-') {
      fileName = arg;
    }
  }

  if (fileName.empty()) {
    std::cerr << "Usage: ./smash [-r] [--smt] [--json [file]] <adl-file>\n"
                 "  -r  object + region disjointness + IR/overlap analysis\n"
                 "  --smt  Z3 check on linear constraints (requires z3)\n"
                 "  --json  write region analysis JSON to file or stdout\n";
    return 1;
  }

  std::ifstream fin(fileName);
  adl::Driver drv(&fin);
  int res = drv.parse();


  if(res == 0) std::cout << "Parsing successful!\n";

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

  if(res == 0) { res = adl::printObjectAttributes(drv); }
  else std::cerr << "Failed printFlowChart()\n";

  if (res == 0 && doRegionAnalysis) {
    res = adl::analyzeObjectDisjointness(drv);
    if (res == 0) res = adl::analyzeRegionDisjointness(drv);
    if (res == 0) {
      adl::region_analysis::AnalysisOptions aopt;
      aopt.runSmt = regionSmt;
      aopt.jsonToStdout = regionJsonStdout;
      aopt.jsonPath = regionJsonPath;
      adl::region_analysis::AnalysisReport areport;
      res = adl::region_analysis::runAnalysis(drv, aopt, areport);
      if (res == 0) res = adl::region_analysis::printReport(areport, aopt);
      if (res == 0 && regionJsonStdout) {
        adl::region_analysis::writeJson(areport, std::cout);
      } else if (res == 0 && !regionJsonPath.empty()) {
        std::ofstream jf(regionJsonPath);
        if (jf) adl::region_analysis::writeJson(areport, jf);
        else {
          std::cerr << "Could not write JSON to " << regionJsonPath << "\n";
          res = 1;
        }
      }
    }
  }

  // if(res == 0) {
  //   res = drv.ast2cuts(&adl::parts,&adl::NodeVars,&adl::ListParts,&adl::NodeCuts,
  //                &adl::BinCuts, &adl::ObjectCuts,
  //                &adl::NameInitializations, &adl::TRGValues,
  //                &adl::ListTables, &adl::cntHistos, &adl::systmap);
  // }
  // std::cout << "\n\nPART: ";
  // for(auto& l: adl::ListParts) {
  //   std::cout << l.first << ", ";
  //   if(l.second[0] == nullptr) std::cout << "NULLPTR";
  // }
  // std::cout << "\n\nOBJ: ";
  // for(auto& l: adl::ObjectCuts) {
  //   std::cout << l.first << ", ";
  //   if(l.second == nullptr) std::cout << "NULLPTR";
  // }
  // std::cout << "\n\nNODE: ";
  // for(auto& l: adl::NodeVars) {
  //   std::cout << l.first << ", ";
  //   if(l.second == nullptr) std::cout << "NULLPTR";
  // }
  // std::cout << "\n\nCUTS: ";
  // for(auto& l: adl::NodeCuts) {
  //   std::cout << l.first << ", ";
  //   if(l.second == nullptr) std::cout << "NULLPTR";
  // }
  // std::cout << "\nParts: " << adl::parts.size() << "\n";
  // std::cout << "NodeVars: " << adl::NodeVars.size() << "\n";
  // std::cout << "ListParts: " << adl::ListParts.size() << "\n";
  // std::cout << "NodeCuts: " << adl::NodeCuts.size() << "\n";
  // std::cout << "BinCuts: " << adl::BinCuts.size() << "\n";
  // std::cout << "ObjectCuts: " << adl::ObjectCuts.size() << "\n";
  // std::cout << "NameInitializations: " << adl::NameInitializations.size() << "\n";
  // std::cout << "TRGValues: " << adl::TRGValues.size() << "\n";
  // std::cout << "ListTables: " << adl::ListTables.size() << "\n";
  // std::cout << "cntHistos: " << adl::cntHistos.size() << "\n";
  // std::cout << "systmap: " << adl::systmap.size() << "\n";

  std::cout << "\n";
  if(res == 0) std::cout << "finished\n";
  else std::cout << "ERROR\n";
  return res;
}
