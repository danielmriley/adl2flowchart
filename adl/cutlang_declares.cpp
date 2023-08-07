

#include "cutlang_declares.h"

namespace adl {

  // Map of strings to function pointers.
  // Write this in a script to generate the cpp code at compile time.
  // std::map<std::string, PropFunction> function_map;
  // std::map<std::string, LFunction> lfunction_map;
  // std::map<std::string, UnFunction> unfunction_map;

  double      specialf( dbxParticle* apart) { return 0.0; }
  double      Qof( dbxParticle* apart) { return 0.0; }
  double      Mof( dbxParticle* apart) { return 0.0; }
  double      Eof( dbxParticle* apart) { return 0.0; }
  double      Pof( dbxParticle* apart) { return 0.0; }
  double     Pzof( dbxParticle* apart) { return 0.0; }
  double     Ptof( dbxParticle* apart) { return 0.0; }
  double PtConeof( dbxParticle* apart) { return 0.0; }
  double EtConeof( dbxParticle* apart) { return 0.0; }
  double AbsEtaof( dbxParticle* apart) { return 0.0; }
  double    Etaof( dbxParticle* apart) { return 0.0; }
  double    Rapof( dbxParticle* apart) { return 0.0; }
  double    Phiof( dbxParticle* apart) { return 0.0; }
  double 	pdgIDof( dbxParticle* apart) { return 0.0; }
  double flavorof( dbxParticle* apart) { return 0.0; }
  double MsoftDof( dbxParticle* apart) { return 0.0; }
  double  DeepBof( dbxParticle* apart) { return 0.0; }
  double   isBTag( dbxParticle* apart) { return 0.0; }
  double isTauTag( dbxParticle* apart) { return 0.0; }
  double isTight ( dbxParticle* apart) { return 0.0; }
  double isMedium( dbxParticle* apart) { return 0.0; }
  double isLoose ( dbxParticle* apart) { return 0.0; }
  double   tau1of( dbxParticle* apart) { return 0.0; }
  double   tau2of( dbxParticle* apart) { return 0.0; }
  double   tau3of( dbxParticle* apart) { return 0.0; }
  double    dxyof( dbxParticle* apart) { return 0.0; }
  double   edxyof( dbxParticle* apart) { return 0.0; }
  double     dzof( dbxParticle* apart) { return 0.0; }
  double    edzof( dbxParticle* apart) { return 0.0; }
  double     vxof( dbxParticle* apart) { return 0.0; }
  double     vyof( dbxParticle* apart) { return 0.0; }
  double     vzof( dbxParticle* apart) { return 0.0; }
  double     vtof( dbxParticle* apart) { return 0.0; }
  double    vtrof( dbxParticle* apart) { return 0.0; }
  double  sieieof( dbxParticle* apart) { return 0.0; }
  double sub1btagof( dbxParticle* apart) { return 0.0; }
  double sub2btagof( dbxParticle* apart) { return 0.0; }
  double mvalooseof( dbxParticle* apart) { return 0.0; }
  double mvatightof( dbxParticle* apart) { return 0.0; }
  double relisoof( dbxParticle* apart) { return 0.0; }
  double isZcandid ( dbxParticle* apart) { return 0.0; }
  double relisoallof( dbxParticle* apart) { return 0.0; }
  double pfreliso03allof( dbxParticle* apart) { return 0.0; }
  double iddecaymodeof( dbxParticle* apart) { return 0.0; }
  double idisotightof( dbxParticle* apart) { return 0.0; }
  double idantieletightof( dbxParticle* apart) { return 0.0; }
  double idantimutightof( dbxParticle* apart) { return 0.0; }
  double tightidof( dbxParticle* apart) { return 0.0; }
  double    puidof( dbxParticle* apart) { return 0.0; }
  double genpartidxof( dbxParticle* apart) { return 0.0; }
  double decaymodeof( dbxParticle* apart) { return 0.0; }
  double tauisoof( dbxParticle* apart) { return 0.0; }
  double softIdof( dbxParticle* apart) { return 0.0; }
  double CCountof( dbxParticle* apart) { return 0.0; }
  double    nbfof( dbxParticle* apart) { return 0.0; }
  double IsoVarof( dbxParticle* apart) { return 0.0; }
  double MiniIsoVarof( dbxParticle* apart) { return 0.0; }
  double averageMuof( dbxParticle* apart) { return 0.0; }
  double truthMatchProbof( dbxParticle* apart) { return 0.0; }
  double truthIDof( dbxParticle* apart) { return 0.0; }
  double truthParentIDof( dbxParticle* apart) { return 0.0; }
  double IDXof( dbxParticle* apart) { return 0.0; }

  // LFuncs
  double dR  (dbxParticle* apart,dbxParticle* apart2) { return 0.0; }
  double dPhi(dbxParticle* apart,dbxParticle* apart2) { return 0.0; }
  double dEta(dbxParticle* apart,dbxParticle* apart2) { return 0.0; }

  // sfunction_map[]
  double hstep(double x) { return 0.0; }
  double delta(double x) { return 0.0; }
  double abs(double x) { return 0.0; }
  double sqrt(double x) { return 0.0; }
  double sin(double x) { return 0.0; }
  double cos(double x) { return 0.0; }
  double tan(double x) { return 0.0; }
  double sinh(double x) { return 0.0; }
  double cosh(double x) { return 0.0; }
  double tanh(double x) { return 0.0; }
  double exp(double x) { return 0.0; }
  double log(double x) { return 0.0; }

  double none(AnalysisObjects* ao, std::string s, float id) { return 0.0; }
  double all(AnalysisObjects* ao, std::string s, float id) { return 0.0; }
  double uweight(AnalysisObjects* ao, std::string s, float value) { return 0.0; }
  double lepsf(AnalysisObjects* ao, std::string s, float value) { return 0.0; }
  double btagsf(AnalysisObjects* ao, std::string s, float value) { return 0.0; }
  double xslumicorrsf(AnalysisObjects* ao, std::string s, float value) { return 0.0; }
  double count(AnalysisObjects* ao, std::string s, float id) { return 0.0; }
  double getIndex(AnalysisObjects* ao, std::string s, float id) { return 0.0; } // new internal function
  double met   (AnalysisObjects* ao, std::string s, float id) { return 0.0; }
  double metsig(AnalysisObjects* ao, std::string s, float id) { return 0.0; }
  double hlt_iso_mu(AnalysisObjects* ao, std::string s, float id) { return 0.0; }
  double hlt_trg(AnalysisObjects* ao, std::string s, float id) { return 0.0; }
  double ht(AnalysisObjects* ao, std::string s, float id) { return 0.0; }

  // BinaryNode functions
  double add(double left, double right) { return left + right; }
  double mult(double left, double right) { return left - right; }
  double sub(double left, double right) { return left * right; }
  double div(double left, double right) { return left / right; }

  //double power ALREADY EXIST
  double lt(double left, double right) { return 0.0; }
  double le(double left, double right) { return 0.0; }
  double ge(double left, double right) { return 0.0; }
  double gt(double left, double right) { return 0.0; }
  double eq(double left, double right) { return 0.0; }
  double ne(double left, double right) { return 0.0; }
  double LogicalAnd(double left, double right) { return 0.0; }
  double LogicalOr(double left, double right) { return 0.0; }
  double mnof(double left, double right) { return 0.0; }
  double mxof(double left, double right) { return 0.0; }

  double unaryMinu(double left) { return 0.0; }
  double LogicalNot(double condition) { return 0.0; }

  // Object creation functions
  void updateParticles( Node* cutIt, std::vector<myParticle *>* particles, int ipart, std::string name) {}
  int getCollectionSize(int t2, std::string base_collection2, AnalysisObjects *ao ) { return 1; }
  void createNewJet   (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename) {}
  void createNewFJet  (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename) {}
  void createNewEle   (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename) {}
  void createNewMuo   (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename) {}
  void createNewTau   (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename) {}
  void createNewPho   (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename) {}
  void createNewCombo (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename) {}
  void createNewParti (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename) {}
  void createNewTruth (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename) {}
  void createNewTrack (AnalysisObjects* ao,std::vector<Node*> *criteria,std::vector<myParticle *>* particles, std::string name, std::string basename) {}

  std::vector<TLorentzVector> fmegajets(std::vector<TLorentzVector> myjets, int p1) {
  // p1 is unused, not to be deleted for compatibility reasons.
      std::vector<TLorentzVector> mynewjets;
      return mynewjets;

  }

    // MR
  double fMR(std::vector<TLorentzVector> j){
      double temp;
      return temp;
  }

    // MTR
  double fMTR(std::vector<TLorentzVector> j, TVector2 amet){
      double temp;
      return temp;
  }
  double fMTR2(std::vector<TLorentzVector> j, TLorentzVector amet){
    double temp;
    return temp;
  }

    // MT
  double fMT(std::vector<TLorentzVector> v){
    return 0.0;
  }

  double userfuncA(AnalysisObjects* ao, std::string s, int id, std::vector<TLorentzVector> (*func)(std::vector<TLorentzVector> jets, int p1) ) { return 0.0; }
  double userfuncB(AnalysisObjects* ao, std::string s, int id, double (*func)(std::vector<TLorentzVector> jets ) ) { return 0.0; }
  double userfuncC(AnalysisObjects* ao, std::string s, int id, double (*func)(std::vector<TLorentzVector> jets, TVector2 amet ) ) { return 0.0; }
  double userfuncD(AnalysisObjects* ao, std::string s, int id, TLorentzVector alv, double (*func)(std::vector<TLorentzVector> jets, TLorentzVector amet )) { return 0.0; }
  double userfuncE(AnalysisObjects* ao, std::string s, int id, TLorentzVector l1, TLorentzVector l2,  TLorentzVector m1,
                                                              double (*func)(TLorentzVector la, TLorentzVector lb, TLorentzVector amet ) ) { return 0.0; }
  double userfuncF(AnalysisObjects* ao, std::string s, int id, double l1, double l2,  double m1, double l3,
                                                              double (*func)(double la, double lb, double amet, double lab ) ) { return 0.0; }


}
