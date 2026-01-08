#include "elbo_sdk/uid.h"

#include <random>
#include <string>

namespace elbo_sdk {

std::string new_uid16() {
    static constexpr char kHex[] = "0123456789abcdef";

    std::random_device rd;
    std::uniform_int_distribution<int> dist(0, 15);

    std::string out;
    out.reserve(16);
    for (int i = 0; i < 16; ++i) {
        out.push_back(kHex[dist(rd)]);
    }
    return out;
}

} // namespace elbo_sdk
