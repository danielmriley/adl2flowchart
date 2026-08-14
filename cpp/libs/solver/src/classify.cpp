#include "adl2/solver/solver.hpp"

#include <fcntl.h>
#include <sys/wait.h>
#include <unistd.h>

#include <cerrno>
#include <cstring>
#include <sstream>
#include <string>

namespace adl2::solver {
namespace {

std::string trim_copy(const std::string& s) {
  std::size_t b = 0;
  while (b < s.size() &&
         (s[b] == ' ' || s[b] == '\t' || s[b] == '\r' || s[b] == '\n')) {
    ++b;
  }
  std::size_t e = s.size();
  while (e > b &&
         (s[e - 1] == ' ' || s[e - 1] == '\t' || s[e - 1] == '\r' ||
          s[e - 1] == '\n')) {
    --e;
  }
  return s.substr(b, e - b);
}

std::string first_error_line(const std::string& output) {
  std::istringstream in(output);
  std::string line;
  while (std::getline(in, line)) {
    if (line.find("error") != std::string::npos) return trim_copy(line);
  }
  return "";
}

}  // namespace

SatResult classify(const std::string& output) {
  if (output.find("(error") != std::string::npos ||
      output.find("error \"") != std::string::npos) {
    return SatResult::unknown(std::string(SOLVER_ERROR) + ": " +
                              first_error_line(output));
  }
  {
    std::istringstream in(output);
    std::string line;
    while (std::getline(in, line)) {
      if (trim_copy(line) == "unsupported") {
        return SatResult::unknown(UNSUPPORTED);
      }
    }
  }
  {
    std::istringstream in(output);
    std::string line;
    while (std::getline(in, line)) {
      const std::string t = trim_copy(line);
      if (t == "sat") return SatResult::sat();
      if (t == "unsat") return SatResult::unsat();
      if (t == "unknown") return SatResult::unknown(ANSWERED_UNKNOWN);
      if (t == "timeout") return SatResult::unknown(TIMEOUT);
    }
  }
  return SatResult::unknown(std::string(NO_ANSWER) + ": " + trim_copy(output));
}

bool subprocess_available(const std::string& cmd) {
  if (cmd.empty()) return false;
  int devnull = ::open("/dev/null", O_RDWR);
  if (devnull < 0) return false;
  pid_t pid = ::fork();
  if (pid < 0) {
    ::close(devnull);
    return false;
  }
  if (pid == 0) {
    ::dup2(devnull, STDIN_FILENO);
    ::dup2(devnull, STDOUT_FILENO);
    ::dup2(devnull, STDERR_FILENO);
    if (devnull > STDERR_FILENO) ::close(devnull);
    ::execlp(cmd.c_str(), cmd.c_str(), "-version", static_cast<char*>(nullptr));
    ::_exit(127);
  }
  ::close(devnull);
  int status = 0;
  if (::waitpid(pid, &status, 0) < 0) return false;
  return WIFEXITED(status) && WEXITSTATUS(status) == 0;
}

int module_anchor() { return 4; }

}  // namespace adl2::solver
