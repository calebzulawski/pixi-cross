target_platform ?= native

openssl-version-$(target_platform): openssl-version.cpp Makefile pixi.lock
	$(CXX) $(CPPFLAGS) $(CXXFLAGS) $(LDFLAGS) -o $@ $< -lcrypto
