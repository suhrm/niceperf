use common;
use polling;
use quinn_proto;
mod test {
    use super::*;
    #[test]
    fn test() {
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn quinn_test() {
        let mut client_config = common::configure_client(None);
        let mut server_config = common::configure_server();

        let client_ep_cfg = quinn_proto::EndpointConfig::default();
        let server_ep_cfg = quinn_proto::EndpointConfig::default();

        let mut client_ep =
            quinn_proto::Endpoint::new(client_config, None, false);
        let mut server_ep = quinn_proto::Endpoint::new(
            server_ep_cfg,
            Some(server_ep_cfg),
            true,
        );

        let con_handle = client_ep
            .connect(client_config, "127.0.0.1:12345", "localhost")
            .unwrap();
    }
}
