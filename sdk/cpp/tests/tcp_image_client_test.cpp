#include <daedalus_sim_sdk/tcp_image_client.hpp>

using namespace daedalus::sim::sdk::v1;

int main() {
  TcpImageClient client({"127.0.0.1", 1});
  if (client.connected()) return 1;
  const auto frame = client.latest();
  if (frame || frame.status.error != ClientError::NotReady) return 2;
  client.close();
  return client.connected() ? 3 : 0;
}
