#include <iostream>
#include <vector>

int main() {
    std::vector<int> values{1, 2, 3};
    for (auto value : values) {
        std::cout << value << '\n';
    }
}
