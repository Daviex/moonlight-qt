#include "moonlight_shell.h"

#include <slint.h>

#include <iostream>
#include <memory>
#include <vector>

using slint::SharedString;
using slint::VectorModel;

int main()
{
    auto shell = MoonlightShell::create();

    std::vector<HostEntry> hosts = {
        HostEntry {
            SharedString("Gaming PC"),
            SharedString("Online"),
            true,
            false,
        },
        HostEntry {
            SharedString("Living Room PC"),
            SharedString("Offline"),
            true,
            false,
        },
        HostEntry {
            SharedString("Steam Deck Test Host"),
            SharedString("Pairing required"),
            false,
            false,
        },
    };

    shell->set_hosts(std::make_shared<VectorModel<HostEntry>>(hosts));
    shell->on_host_accepted([](int index) {
        std::cout << "Accepted host index " << index << std::endl;
    });
    shell->on_settings_requested([]() {
        std::cout << "Settings requested" << std::endl;
    });

    return shell->run();
}
