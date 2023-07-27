#ifndef CUTLANGDEC_H
#define CUTLANGDEC_H

#include <vector>
#include <list>
#include <map>
#include <utility>
#include <string>

namespace adl {
  class TLorentzVector;
  class AnalysisObjects;
  class TVector2;

  class dbxParticle {
  public:
    dbxParticle() {}
  };

  typedef double (*UnFunction)(double);
  typedef double (*LFunction)(dbxParticle*,dbxParticle*);
  typedef double (*PropFunction)(dbxParticle*);
  typedef double (*SFunction)(AnalysisObjects*, std::string, float);

  struct myParticle {
    int type;
    int index;
    std::string collection;
  };

  class Node {
  public:
    int type;
    std::string name;
  };

  class BinaryNode : public Node {
  private:
    double (*f)(double, double);
  public:
    BinaryNode(double (*func)(double, double), Node* l, Node* r, std::string s) { }
  };

  class UnaryAONode : public Node {
  private:
    double (*f)(double);
  public:
    UnaryAONode(double (*func)(double), Node* l, std::string s) { }
  };

  class FuncNode : public Node{
  private:
    double (*f)(dbxParticle* apart);

  public:
      FuncNode(double (*func)(dbxParticle* apart ),std::vector<myParticle*> input,  std::string s,
               Node *objectNodea = NULL, std::string as="", Node *objectNodeb = NULL, Node *objectNodec = NULL, Node *objectNoded = NULL) {}
  };

  class LoopNode : public Node{
  private:
    double (*f)(std::vector<double>);

  public:
    LoopNode(std::vector<bool> (*func)(std::vector<double>), Node* l, std::string s) { }
    LoopNode(double (*func)(std::vector<double>), Node* l, std::string s) { }
    LoopNode(double (*func)(std::vector<double>), std::vector<Node*> ls, std::string s) { }
  };

  class LFuncNode : public FuncNode{
  private:
    double (*f2)(dbxParticle* part1,dbxParticle* part2);

  public:
    LFuncNode(double (*func)(dbxParticle* part1,dbxParticle* part2),std::vector<myParticle*> input1,std::vector<myParticle*> input2,std::string s,
              Node *objectNodea = NULL, Node *objectNodeb = NULL, Node *objectNodec = NULL, Node *objectNoded = NULL): FuncNode(NULL,input1,s, objectNodea,"", objectNodeb, objectNodec, objectNoded) {}
  };

  class SFuncNode : public Node {
  private:
  public:
      SFuncNode(double (*func)(AnalysisObjects* ao, std::string s, float val),
                float val,
                std::string s,
                Node *objectNodeA = NULL, Node *objectNodeB = NULL){
  }
      SFuncNode(double (*func)(AnalysisObjects* ao, std::string s, float val),
                Node *child, std::string s,
                Node *objectNodeA = NULL, Node *objectNodeB = NULL){
  }

  //------------------------- g1 with userfuncA
      SFuncNode(double (*func)(AnalysisObjects* ao, std::string s, int id, std::vector<TLorentzVector> (*gunc) (std::vector<TLorentzVector> jets, int p1)),
                std::vector<TLorentzVector> (*tunc) (std::vector<TLorentzVector> jets, int p1),
                        int id,
                 std::string s,
                 Node *objectNodeA = NULL, Node *objectNodeB = NULL){
  }
  SFuncNode(double (*func)(AnalysisObjects* ao, std::string s, int id, double (*gunc) (std::vector<TLorentzVector> jets)),
                double (*tunc) (std::vector<TLorentzVector> jets),
                        int id,
                 std::string s,
                 Node *objectNodeA = NULL, Node *objectNodeB = NULL){

  }
  SFuncNode(double (*func)(AnalysisObjects* ao, std::string s, int id, double (*gunc) (std::vector<TLorentzVector> jets, TVector2 amet)),
                double (*tunc) (std::vector<TLorentzVector> jets, TVector2 amet),
                        int id,
                 std::string s,
                 Node *objectNodeA = NULL, Node *objectNodeB = NULL){
  }
  SFuncNode(double (*func)(AnalysisObjects* ao, std::string s, int id, TLorentzVector alv, double (*gunc) (std::vector<TLorentzVector> jets, TLorentzVector amet)),
                double (*tunc) (std::vector<TLorentzVector> jets, TLorentzVector amet),
                        int id,
                 std::string s,
                 std::vector<myParticle*> input,
                 Node *objectNodeA = NULL, Node *objectNodeB = NULL){
  }
  SFuncNode(double (*func)(AnalysisObjects* ao, std::string s, int id, TLorentzVector a1, TLorentzVector a2, TLorentzVector b1, double (*gunc) (TLorentzVector lep1, TLorentzVector lep2, TLorentzVector amet)),
                double (*tunc) (TLorentzVector lep1, TLorentzVector lep2, TLorentzVector amet),
                        int id,
                 std::string s,
                 std::vector<myParticle*> input1,
                 std::vector<myParticle*> input2,
                 std::vector<myParticle*> input3,
                 Node *objectNodeA = NULL, Node *objectNodeB = NULL){
  }
  //--------------------------------g6
  SFuncNode(double (*func)(AnalysisObjects* ao, std::string s, int id, double pt1, double pt2, double m1, double pt3, double (*gunc)(double a1, double a2, double a3, double a4)),
                double (*tunc) (double a1, double a2, double a3, double a4),
                        int id,
                 std::string s,
                    double apt1, double apt2, double apt3, double apt4,
                 Node *objectNodeA = NULL, Node *objectNodeB = NULL){
  }
  };

  class ObjectNode : public Node {
  private:
    //not sure if we need to use it---------ObjectNode* previous;
    std::vector<Node*> criteria;
    //still need to add something to save the modifed AO
    std::vector<myParticle *> particles;//used to collect particle pointers to be changed
  public:
    ObjectNode(std::string id, Node* previous, void (* func) (AnalysisObjects* ao,std::vector<Node*>* criteria,
               std::vector<myParticle *>* particles, std::string name, std::string basename ), std::vector<Node*> criteria,  std::string s ) {}
  };

  class ValueNode : public Node {
  private:
      double value;
      bool pval;
  public:
      ValueNode(double v=0) {}
      ValueNode(std::string evtvar) {}
  };

  class IfNode : public Node {
  private:
      Node * condition;
  public:
      IfNode(Node* c,Node* l, Node* r,  std::string s ){
        
      }
  };

  struct cntHisto {
    std::string cH_name;
    std::string cH_title;
    std::vector<float> cH_means;
    std::vector<float> cH_StatErr_p;
    std::vector<float> cH_StatErr_n;
    std::vector<float> cH_SystErr_p;
    std::vector<float> cH_SystErr_n;
    bool cH_StatErr;
    bool cH_SystErr;
  };

  enum particleType{
   none_t=0,
   electron_t=1,
   jet_t=2,
   bjet_t=3,
   lightjet_t=4,
   muonlikeV_t=5,
   electronlikeV_t=6,
   pureV_t=7,
   photon_t=8,
   fjet_t=9,
   truth_t=10,
   tau_t=11,
   muon_t=12,
   track_t=19,
   combo_t=20,
   consti_t=21
  };

  extern int cutcount;
  extern int bincount;
  static std::string tmp;
  static int pnum;
  static int dnum;
  static int objIndex = 6213;
  static std::vector<float> tmpBinlist;
  static std::vector<float> tmpBoxlist;
  static std::vector<myParticle*> CombiParticle;
  static std::vector<myParticle*> TmpParticle;
  static std::vector<myParticle*> AliasList;
  static std::vector<myParticle*> TmpParticle1;//to be used for list of 2 particles
  static std::vector<myParticle*> TmpParticle2;//to be used for list of 3 particles
  static std::vector<Node*> TmpCriteria;
  static std::vector<Node*> TmpIDList;
  static std::vector<Node*> VariableList;
  static std::vector<float> chist_a, chist_stat_p, chist_stat_n, chist_syst_p, chist_syst_n;


  static std::list<std::string> parts; //for def of particles as given by user
  static std::map<std::string,Node*> NodeVars;//for variable defintion
  static std::map<std::string,std::vector<myParticle*> > ListParts;//for particle definition
  static std::map<std::string,std::pair<std::vector<float>,bool> > ListTables;//for table definition
  static std::map<std::string, std::vector<cntHisto> > cntHistos;
  static std::map<int, std::vector<std::string> > systmap;
  static std::map<int,Node*> NodeCuts;//cuts and histos
  static std::map<int,Node*> BinCuts;//binning
  static std::map<std::string,Node*> ObjectCuts;//cuts for user defined objects
  static std::vector<std::string> NameInitializations;
  static std::vector<int> TRGValues;
  static std::vector<double> bincounts;
  // Funcs
  double      specialf( dbxParticle* apart);
  double      Qof( dbxParticle* apart);
  double      Mof( dbxParticle* apart);
  double      Eof( dbxParticle* apart);
  double      Pof( dbxParticle* apart);
  double     Pzof( dbxParticle* apart);
  double     Ptof( dbxParticle* apart);
  double PtConeof( dbxParticle* apart);
  double EtConeof( dbxParticle* apart);
  double AbsEtaof( dbxParticle* apart);
  double    Etaof( dbxParticle* apart);
  double    Rapof( dbxParticle* apart);
  double    Phiof( dbxParticle* apart);
  double 	pdgIDof( dbxParticle* apart);
  double flavorof( dbxParticle* apart);
  double MsoftDof( dbxParticle* apart);
  double  DeepBof( dbxParticle* apart);
  double   isBTag( dbxParticle* apart);
  double isTauTag( dbxParticle* apart);
  double isTight ( dbxParticle* apart);
  double isMedium( dbxParticle* apart);
  double isLoose ( dbxParticle* apart);
  double   tau1of( dbxParticle* apart);
  double   tau2of( dbxParticle* apart);
  double   tau3of( dbxParticle* apart);
  double    dxyof( dbxParticle* apart);
  double   edxyof( dbxParticle* apart);
  double     dzof( dbxParticle* apart);
  double    edzof( dbxParticle* apart);
  double     vxof( dbxParticle* apart);
  double     vyof( dbxParticle* apart);
  double     vzof( dbxParticle* apart);
  double     vtof( dbxParticle* apart);
  double    vtrof( dbxParticle* apart);
  double  sieieof( dbxParticle* apart);
  double sub1btagof( dbxParticle* apart);
  double sub2btagof( dbxParticle* apart);
  double mvalooseof( dbxParticle* apart);
  double mvatightof( dbxParticle* apart);
  double relisoof( dbxParticle* apart);
  double isZcandid ( dbxParticle* apart);
  double relisoallof( dbxParticle* apart);
  double pfreliso03allof( dbxParticle* apart);
  double iddecaymodeof( dbxParticle* apart);
  double idisotightof( dbxParticle* apart);
  double idantieletightof( dbxParticle* apart);
  double idantimutightof( dbxParticle* apart);
  double tightidof( dbxParticle* apart);
  double    puidof( dbxParticle* apart);
  double genpartidxof( dbxParticle* apart);
  double decaymodeof( dbxParticle* apart);
  double tauisoof( dbxParticle* apart);
  double softIdof( dbxParticle* apart);
  double CCountof( dbxParticle* apart);
  double    nbfof( dbxParticle* apart);
  double IsoVarof( dbxParticle* apart);
  double MiniIsoVarof( dbxParticle* apart);
  double averageMuof( dbxParticle* apart);
  double truthMatchProbof( dbxParticle* apart);
  double truthIDof( dbxParticle* apart);
  double truthParentIDof( dbxParticle* apart);
  double IDXof( dbxParticle* apart);

  // LFuncs
  double dR  (dbxParticle* apart,dbxParticle* apart2);
  double dPhi(dbxParticle* apart,dbxParticle* apart2);
  double dEta(dbxParticle* apart,dbxParticle* apart2);

  // SFuncs
  double none(AnalysisObjects* ao, std::string s, float id);
  double all(AnalysisObjects* ao, std::string s, float id);
  double uweight(AnalysisObjects* ao, std::string s, float value);
  double lepsf(AnalysisObjects* ao, std::string s, float value);
  double btagsf(AnalysisObjects* ao, std::string s, float value);
  double xslumicorrsf(AnalysisObjects* ao, std::string s, float value);
  double count(AnalysisObjects* ao, std::string s, float id);
  double getIndex(AnalysisObjects* ao, std::string s, float id); // new internal function
  double met   (AnalysisObjects* ao, std::string s, float id);
  double metsig(AnalysisObjects* ao, std::string s, float id);
  double hlt_iso_mu(AnalysisObjects* ao, std::string s, float id);
  double hlt_trg(AnalysisObjects* ao, std::string s, float id);
  double ht(AnalysisObjects* ao, std::string s, float id);


  // Unary
  double hstep(double x);
  double delta(double x);
  double abs(double x);
  double sqrt(double x);
  double sin(double x);
  double cos(double x);
  double tan(double x);
  double sinh(double x);
  double cosh(double x);
  double tanh(double x);
  double exp(double x);
  double log(double x);

  double unaryMinu(double left);
  double LogicalNot(double condition);


  // BinaryNode functions
  double add(double left, double right);
  double mult(double left, double right);
  double sub(double left, double right);
  double div(double left, double right);

  //double power ALREADY EXIST
  double lt(double left, double right);
  double le(double left, double right);
  double ge(double left, double right);
  double gt(double left, double right);
  double eq(double left, double right);
  double ne(double left, double right);
  double LogicalAnd(double left, double right);
  double LogicalOr(double left, double right);
  double mnof(double left, double right);
  double mxof(double left, double right);

  // Object creation functions
  void updateParticles( Node* cutIt, std::vector<myParticle *>* particles, int ipart, std::string name);
  int getCollectionSize(int t2, std::string base_collection2, AnalysisObjects *ao );
  void createNewJet   (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename);
  void createNewFJet  (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename);
  void createNewEle   (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename);
  void createNewMuo   (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename);
  void createNewTau   (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename);
  void createNewPho   (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename);
  void createNewCombo (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename);
  void createNewParti (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename);
  void createNewTruth (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename);
  void createNewTrack (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename);



  void fillFuncMaps(std::map<std::string, PropFunction> &function_map,
                    std::map<std::string, LFunction> &lfunction_map,
                    std::map<std::string, UnFunction> &unfunction_map,
                    std::map<std::string, SFunction> &sfunction_map) ;
}

#endif
