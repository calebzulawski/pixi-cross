#include <openssl/crypto.h>
#include <cstdio>

int main() {
    std::puts(OpenSSL_version(OPENSSL_VERSION));
}
