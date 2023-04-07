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

  int Driver::visitAST(int (*f)(ExprVector& _ast)) {
    return f(ast);
  }

  int Driver::setTables() {
    for(int i = 0; i < ast.size(); i++) {
      std::string token = ast[i]->getToken();
      if(token == "DEFINE") {
        addDefine(ast[i]->getId());
      }
      else if(token == "OBJECT") {
        addObject(ast[i]->getId());
      }
      else if(token == "REGION") {
        addRegion(ast[i]->getId());
        // add region's vars to "region vars" table.
      }
    }
    return 0;
  }

  int Driver::checkObjectTable(std::string id) {
    for(auto e: objectTable) {
      if(e == id) {
        std::cout << "Object declared\n";
        return 0;
      }
    }
    std::cout << "ERROR: Object NOT declared\n";
    return 1;
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
    loc+=l;
  }

  unsigned int Driver::location() {
      return loc;
  }

  int Driver::ast2cuts(std::list<std::string> *parts,std::map<std::string,Node*>* NodeVars,
               std::map<std::string, std::vector<myParticle*> >* ListParts,
               std::map<int,Node*>* NodeCuts,
               std::map<int,Node*>* BinCuts, std::map<std::string,Node*>* ObjectCuts,
               std::vector<std::string>* Initializations,
               std::vector<int>* TRGValues, std::map<std::string,
               std::pair<std::vector<float>, bool> >* ListTables,
               std::map<std::string, std::vector<cntHisto> >*cntHistos,
               std::map<int, std::vector<std::string> > *systmap)
  {
    for(auto& a: ast) { // Loop through the AST and fill in data structures.
      if(a->getToken() == "DEFINE") {
        VarNode* varnode = static_cast<VarNode*>(static_cast<DefineNode*>(a)->getVar());
        std::string name = varnode->getId();
        std::cout << "DEF NAME: " << name << "\n";
        pnum = 0;

        std::map<std::string,std::vector<myParticle*>>::iterator it;
        it = ListParts->find(name);

        std::map<std::string,Node*>::iterator ito;
        ito = ObjectCuts->find(name);

        if(ito != ObjectCuts->end()) { // Object found
          int otype = ito->second->type;
          myParticle* mp = new myParticle;

          if(otype == electron_t) {
            std::cout << "Electron type\n";
            mp->type = electron_t;
            mp->index = objIndex;
            mp->collection = name;
          }
          if(otype == muon_t) {
            std::cout << "Muon type\n";
            mp->type = electron_t;
            mp->index = objIndex;
            mp->collection = name;
          }
          if(otype == tau_t) {
            std::cout << "Tau type\n";
            mp->type = electron_t;
            mp->index = objIndex;
            mp->collection = name;
          }
          if(otype == jet_t) {
            std::cout << "Jet type\n";
            mp->type = electron_t;
            mp->index = objIndex;
            mp->collection = name;
          }
        }

        Expr* body = static_cast<DefineNode*>(a)->getBody();
        std::cout << "BODY TOKEN: " << body->getToken() << "\n";
        parts->push_back(name + " : " + "");
      }
      if(a->getToken() == "REGION") {
        RegionNode *regionnode = static_cast<RegionNode*>(a);
        std::string name = regionnode->getId();
        std::cout << "REG NAME: " << name << "\n";

      }
      if(a->getToken() == "OBJECT") {
        ObjectNode *objectnode = static_cast<ObjectNode*>(a);
        std::string name = objectnode->getId();
        std::cout << "OBJ NAME: " << name << "\n";

      }
    }
    return 0;
  }
} // end namespace adl
