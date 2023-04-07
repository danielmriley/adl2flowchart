#ifndef CUTLANGDEC_H
#define CUTLANGDEC_H

#include <vector>
#include <list>
#include <map>
#include <utility>
#include <string>

namespace adl {
  class Node {
  public:
    int type;
    std::string name;
  };

  struct myParticle{
    int type;
    int index;
    std::string collection;
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
}

#endif
