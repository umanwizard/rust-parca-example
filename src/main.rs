use std::time::Duration;

pub mod profilestore {
    tonic::include_proto!("parca.profilestore.v1alpha1");
}

use jemalloc_pprof::PROF_CTL;
use profilestore::profile_store_service_client::ProfileStoreServiceClient;
use tonic::metadata::MetadataValue;
use tonic::transport::Channel;
use tonic::Request;

use crate::profilestore::{Label, LabelSet, RawProfileSeries, RawSample, WriteRawRequest};

// Configure jemalloc to allow profiling
#[allow(non_upper_case_globals)]
#[export_name = "malloc_conf"]
pub static malloc_conf: &[u8] = b"prof:true,prof_active:true,lg_prof_sample:19\0";

#[tokio::main]
async fn main() {
    let bearer_token = "(SOME BEARER TOKEN)";

    tokio::select! {
        _ = write_profile_loop(Duration::from_secs(10), bearer_token) => {
            eprintln!("write profile loop unexpectedly quit")
        },
        _ = allocator_loop() => {
            eprintln!("allocator loop unexpectedly quit")
        }
    }
}

// allocate 1 MB every 1s and leak it
async fn allocator_loop() {
    let mut interval = tokio::time::interval(Duration::from_secs(1));

    loop {
        interval.tick().await;
        let leaked = Vec::<u8>::with_capacity(1024 * 1024);
        std::mem::forget(leaked)
    }
}

async fn get_pprof() -> Vec<u8> {
    let prof_ctl = PROF_CTL.as_ref().unwrap();
    let mut borrow = prof_ctl.lock().await;
    borrow.dump_pprof().unwrap()
}

// write a profile every `interval`
async fn write_profile_loop(interval: Duration, token: &str) {
    let mut interval = tokio::time::interval(interval);
    let tls = tonic::transport::ClientTlsConfig::new();
    let channel = Channel::from_static("https://grpc.polarsignals.com:443")
        .tls_config(tls)
        .unwrap()
        .connect()
        .await
        .unwrap();
    let token: MetadataValue<_> = format!("Bearer {token}").parse().unwrap();
    let mut client =
        ProfileStoreServiceClient::with_interceptor(channel, move |mut req: Request<()>| {
            req.metadata_mut().insert("authorization", token.clone());
            Ok(req)
        });
    loop {
        interval.tick().await;
        let pprof = get_pprof().await;
        eprint!("Uploading profile of size {} ...", pprof.len());

        let request = tonic::Request::new(WriteRawRequest {
            series: vec![RawProfileSeries {
                samples: vec![RawSample {
                    raw_profile: pprof,
                    executable_info: vec![],
                }],
                labels: Some(LabelSet {
                    labels: vec![
                        Label {
                            name: "__name__".to_string(),
                            value: "outstanding_allocations".to_string(),
                        },
                        Label {
                            name: "pid".to_string(),
                            value: format!("{}", std::process::id()),
                        },
                    ],
                }),
            }],
            ..Default::default()
        });
        client.write_raw(request).await.unwrap();
        eprintln!(" ...done!")
    }
}
