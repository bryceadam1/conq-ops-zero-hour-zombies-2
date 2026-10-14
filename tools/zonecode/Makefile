SRCS := $(shell find s/ZoneCodeGenerator s/ZoneCodeGeneratorLib s/Parser s/Utils -name '*.cpp')
OBJS := $(patsubst s/%.cpp,o/%.o,$(SRCS))
CXXFLAGS := -DARCH_x86 -Igen -std=c++23 -O1 -w -Is/Utils -Is/Parser -Is/ZoneCodeGeneratorLib -Is/ZoneCodeGenerator
zcg: $(OBJS)
	clang++ -o $@ $^
o/%.o: s/%.cpp
	@mkdir -p $(dir $@)
	clang++ $(CXXFLAGS) -c $< -o $@
