#include <fstream>
#include <iostream>
#include <string>

#include "scanner.hpp"
#include "Parser.h"
#include "driver.h"
#include "region_analysis.hpp"
#include "semantic_checks.h"

int main(int argc, char **argv) {
  bool doRegionAnalysis = false;
  bool regionSmt = true;
  bool regionNoSmt = false;
  bool legacyRegionReport = false;
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
    } else if (arg == "--no-smt") {
      doRegionAnalysis = true;
      regionNoSmt = true;
    } else if (arg == "--legacy-region-report") {
      doRegionAnalysis = true;
      legacyRegionReport = true;
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
    std::cerr << "Usage: ./smash [-r] [--no-smt] [--legacy-region-report] [--json [file]] <adl-file>\n"
                 "  -r  object + region IR/SMT analysis (Z3 when installed)\n"
                 "  --no-smt  skip Z3 even if z3 is on PATH\n"
                 "  --legacy-region-report  also print verbose legacy region disjointness block\n"
                 "  --json  write region analysis JSON to file or stdout\n";
    return 1;
  }

  std::ifstream fin(fileName);
  if (!fin.good()) {
    std::cerr << "ERROR: cannot open input file '" << fileName << "'\n";
    return 1;
  }
  adl::Driver drv(&fin);

  int res = drv.parse();
  if (res != 0) {
    std::cerr << "Parsing failed for '" << fileName << "'\n";
    std::cout << "\nERROR\n";
    return res;
  }
  std::cout << "Parsing successful!\n";

  auto fail = [](const char* stage, int rc) {
    std::cerr << "Stage failed: " << stage << "\n";
    std::cout << "\nERROR\n";
    return rc;
  };
  if ((res = drv.setTables()) != 0) return fail("setTables", res);
  if ((res = adl::checkDecl(drv)) != 0) return fail("checkDecl", res);
  if ((res = adl::typeCheck(drv)) != 0) return fail("typeCheck", res);

  // run "dot -Tpdf ast.dot -o ast.pdf" / "dot -Tpdf fc.dot -o fc.pdf"
  res = drv.visitAST(adl::printAST);
  if (res == 0) res = adl::printFlowChart(drv);
  if (res == 0) res = adl::printObjectAttributes(drv);
  if (res != 0) {
    std::cerr << "DOT/attribute output failed\n";
    std::cout << "\nERROR\n";
    return res;
  }

  if (res == 0 && doRegionAnalysis) {
    res = adl::analyzeObjectDisjointness(drv);
    if (res == 0 && legacyRegionReport)
      res = adl::analyzeRegionDisjointness(drv);
    if (res == 0) {
      adl::region_analysis::AnalysisOptions aopt;
      aopt.runSmt = regionSmt;
      aopt.autoSmt = !regionNoSmt;
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

  std::cout << "\n";
  if(res == 0) std::cout << "finished\n";
  else std::cout << "ERROR\n";
  return res;
}
