#include <daedalus_sim_sdk/contest_client.hpp>

using namespace daedalus::sim::sdk::v1;

int main() {
  ContestClientOptions options;
  options.ipc_directory = "/tmp/daedalus-contest-test";
  ContestClient client(options);
  if (client.connected()) return 1;
  if (client.options().scene_control.session_id != "contest-cpp-client") return 2;
  if (client.health().status.error != ClientError::NotReady) return 3;
  if (client.selectScene(ContestScene::ShootingRange).status.error !=
      ClientError::NotReady) return 4;
  if (client.getLatestArmorHit().status.error != ClientError::NotReady) return 5;
  return 0;
}
