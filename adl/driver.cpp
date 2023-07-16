#include "driver.h"

namespace adl {
  std::string toupper(std::string s);

  Driver::Driver() :
    scanner(*this),
    parser(scanner, *this),
    loc(0) {}

  int Driver::parse() {
    loc = 0;
    return parser.parse();
  }

  int Driver::parse(std::string fileName) {
    loc = 0;
    // if(fileName == "") {
    //   scanner.yyin = stdin;
    // }
    // else {
    //   scanner.yyin = fopen(fileName.c_str(), "r");
    // }

    return parser.parse();
  }

  int Driver::visitAST(int (*f)(ExprVector& _ast)) {
    return f(ast);
  }

  void Driver::loadFromLibraries() {
    std::ifstream fin("./adl/ext_objs.txt");
    std::string input;

    while(fin >> input) {
      input = toupper(input);
      addObject(input,std::string("PARENT"));
    }
    fin.close();
  }

  std::string Driver::getBinType(Expr* expr) {
    std::string lhsType, rhsType;
    if(binOpCheck(expr) == 0) {
      BinNode* binExpr = static_cast<BinNode*>(expr);
      lhsType = getBinType(binExpr->getLHS());
      rhsType = getBinType(binExpr->getRHS());
      if(lhsType == rhsType) return lhsType;
      else {
        // std::cout << "ERROR: There is a type mismatch\n";
        return "";
      }
    }
    else return static_cast<VarNode*>(expr)->getType();
  }

  std::string Driver::getObjectDeclType(std::string s) {
    for(auto &o: objectTable) {
      if(o.first == toupper(s)) {
        return o.second;
      }
    }
    return "NOT FOUND";
  }

  int Driver::setTables() {
    loadFromLibraries();
    for(int i = 0; i < ast.size(); i++) {
      std::string token = ast[i]->getToken();
      if(token == "DEFINE") {
        std::string defType;
        if(binOpCheck(ast[i]) == 0) {
          defType = getBinType(ast[i]);
        }
        else if(ast[i]->getToken() == "ID") {
          defType = static_cast<VarNode*>(ast[i])->getType();
        }
        addDefine(ast[i]->getId());
      }
      else if(token == "OBJECT") {
        std::string takeType;
        ExprVector stmnts = static_cast<ObjectNode*>(ast[i])->getStatements();
        for(auto& e: stmnts) {
          if(e->getToken() == "TAKE") {
            CommandNode* cn = static_cast<CommandNode*>(e);
            VarNode* vn = static_cast<VarNode*>(cn->getCondition());
            takeType = vn->getId();
          }
        }
        addObject(ast[i]->getId(),takeType);
      }
      else if(token == "REGION") {
        addRegion(ast[i]->getId());
        // add region's vars to "region vars" table.
      }
      else if(token == "HISTOLIST") {
        addRegion(ast[i]->getId());
        // add region's vars to "region vars" table.
      }
    }
    return 0;
  }

  int Driver::checkObjectTable(std::string id) {
    for(auto& e: objectTable) {
      std::string eUpper = adl::toupper(e.first);
      std::string idUpper = adl::toupper(id);
      if(eUpper == idUpper) {
        // std::cout << "Object " << id << " has been declared\n";
        return 0;
      }
    }
    // std::cout << "ERROR: Object " << id << " NOT declared\n";
    return 1;
  }

  int Driver::checkDefinitionTable(std::string id) {
    for(auto e: definitionTable) {
      std::string eUpper = adl::toupper(e);
      std::string idUpper = adl::toupper(id);
      if(eUpper == idUpper) {
        // std::cout << "Variable " << id << " has been declared\n";
        return 0;
      }
    }
    // std::cout << "ERROR: Variable " << id << " NOT declared\n";
    return 1;
  }

  int Driver::checkRegionTable(std::string id) {
    for(auto e: regionTable) {
      std::string eUpper = adl::toupper(e);
      std::string idUpper = adl::toupper(id);
      if(eUpper == idUpper) {
        // std::cout << "Region " << id << " has been declared\n";
        return 0;
      }
    }
    // std::cout << "ERROR: Region " << id << " NOT declared\n";
    return 1;
  }

  void Driver::addNode(Expr* node) {
    ast.push_back(node);
  }

  int Driver::addDefine(std::string id) {
    // Check that the definition isn't already in the table.
    for(auto e: definitionTable){
      if(e == id) {
        // std::cout << "ERROR: Variable " << id << "  has been previously defined\n";
        return 1;
      }
    }
    definitionTable.push_back(id);
    return 0;
  }

  int Driver::addObject(std::string id,std::string takeType) {
    // Check that the definition isn't already in the table.
    for(auto e: objectTable){
      if(e.first == id) {
        // std::cout << "ERROR: Object " << id << " has been previously defined\n";
        return 1;
      }
    }
    objectTable[id] = takeType;
    return 0;
  }

  int Driver::addRegion(std::string id) {
    // Check that the definition isn't already in the table.
    for(auto e: regionTable){
      if(e == id) {
        // std::cout << "ERROR: Region " << id << "  has been previously defined\n";
        return 1;
      }
    }
    regionTable.push_back(id);
    return 0;
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
    std::cout << "\n==== ast2cuts ====\n\n";
    for(auto& a: ast) { // Loop through the AST and fill in data structures.
      if(a->getToken() == "DEFINE") {
        VarNode* varnode = static_cast<VarNode*>(static_cast<DefineNode*>(a)->getVar());
        std::string name = varnode->getId();
        std::cout << "DEF NAME: " << name << "\n";
        pnum = 0;

        std::map<std::string,std::vector<myParticle*>>::iterator it;
        it = ListParts->find(name);

        if(it != ListParts->end()) { // Define already made.. Something went wrong.
          std::cerr << "Define already made. Something went wrong.\n";
          exit(0);
        }

        Expr* body = static_cast<DefineNode*>(a)->getBody();
        std::cout << "BODY TOKEN: " << body->getToken() << "\n";
        if(body->getToken() == "FUNCTION") {
          parts->push_back(name + " : " + "");
        }
        if(binOpCheck(body) == 0) {
          ExprVector operands;
          std::vector<myParticle*> opersList;
          collectBinOpers(body, operands);
          for(auto& o: operands) {
            std::cout << "operand: " << o->getId() << "\n";
            VarNode* vn = static_cast<VarNode*>(o);
            std::cout << "type: " << vn->getType() << "\n";
            myParticle* a = new myParticle;
            a->type = 10; a->index = 0; a->collection = "TYPE";
            opersList.push_back(a);
          }
          ListParts->insert(std::make_pair(name,opersList));
        }

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
        // std::map<std::string,Node*>::iterator ito;
        // ito = ObjectCuts->find(name);
        //
        // if(ito != ObjectCuts->end()) { // Object found
        //   int otype = ito->second->type;
        //   myParticle* mp = new myParticle;
        //
        //   if(otype == electron_t) {
        //     std::cout << "Electron type\n";
        //     mp->type = electron_t;
        //     mp->index = objIndex;
        //     mp->collection = name;
        //   }
        //   if(otype == muon_t) {
        //     std::cout << "Muon type\n";
        //     mp->type = electron_t;
        //     mp->index = objIndex;
        //     mp->collection = name;
        //   }
        //   if(otype == tau_t) {
        //     std::cout << "Tau type\n";
        //     mp->type = electron_t;
        //     mp->index = objIndex;
        //     mp->collection = name;
        //   }
        //   if(otype == jet_t) {
        //     std::cout << "Jet type\n";
        //     mp->type = electron_t;
        //     mp->index = objIndex;
        //     mp->collection = name;
        //   }
        // }
      }
    }

    for(auto &p: *parts) std::cout << "part:" << p << ", " << "\n";
    return 0;
  }
} // end namespace adl
