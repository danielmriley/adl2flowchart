#include "driver.h"

namespace adl {
  Driver::Driver () :
    scanner(*this),
    parser(scanner, *this),
    loc(0) {}

  int Driver::parse () {
    loc = 0;
    return parser.parse();
  }

  void Driver::addNode(Expr* node) {
    ast.push_back(node);
  }

  void Driver::addDefine(std::string id) {
    definitionTable.push_back(id);
  }

  void Driver::addObject(std::string id) {
    objectTable.push_back(id);
  }

  void Driver::addRegion(std::string id) {
    regionTable.push_back(id);
  }

  void Driver::incrementLocation(unsigned int l) {
    loc++;
  }

  unsigned int Driver::location() {
      return loc;
  }
} // end namespace adl
