pub mod client;
pub mod mapper;
pub mod manager;

pub use manager::FlintlockVMManager;


/// Generated gRPC code
pub mod grpc {
    pub mod flintlock {
        pub mod types {
            tonic::include_proto!("flintlock.types");
        }
    }

    pub mod microvm {
        pub mod services {
            pub mod api {
                pub mod v1alpha1 {
                    tonic::include_proto!("microvm.services.api.v1alpha1");
                }
            }
        }
    }
}
