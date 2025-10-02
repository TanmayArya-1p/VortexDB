use crate::config::GRPCServerConfig;
use crate::service::vectordb::vector_db_client::VectorDbClient;
use crate::service::vectordb::{DenseVector, InsertVectorRequest, PointId, SearchRequest};
use crate::service::{VectorDBService, run_server};
use crate::utils::ServerEndpoint;
use api;
use api::DbConfig;
use index::IndexType;
use prost_types::Struct;
use storage::StorageType;
use tempfile::tempdir;
use tokio;
use tonic::transport::Channel;

// Inspired from https://github.com/hyperium/tonic/discussions/924#discussioncomment-9854088

// TODO: figure out a way to either:
// - assign different ports for different tests; when cargo test is run with multiple threads -> address in use error
// - use a shared instance of the server
// currently tests must be run with --test-threads=1

async fn start_test_server() -> Result<(), Box<dyn std::error::Error>> {
    // using a temporary directory for db datapath
    let temp_dir = tempdir().unwrap();

    let db_config = DbConfig {
        storage_type: StorageType::RocksDb,
        index_type: IndexType::Flat,
        data_path: temp_dir.path().to_path_buf(),
        dimension: 3,
    };

    let config = GRPCServerConfig {
        addr: "127.0.0.1:8080".parse()?,
        root_password: "123".to_string(),
        logging: false,
        db_config,
    };

    let vector_db_api = api::init_api(config.db_config)?;

    let vector_db_service = VectorDBService {
        vector_db: vector_db_api,
        logging: config.logging,
    };

    let listener = tokio::net::TcpListener::bind(config.addr).await?;

    tokio::spawn(async move {
        let _ = run_server(
            vector_db_service,
            ServerEndpoint::Listener(listener),
            config.root_password,
        )
        .await
        .inspect_err(|err| panic!("Could not start test server : {:?}", err));
    });

    Ok(())
}

async fn create_test_client() -> Result<VectorDbClient<Channel>, Box<dyn std::error::Error>> {
    let channel = tonic::transport::Channel::from_static("http://127.0.0.1:8080")
        .connect()
        .await?;
    Ok(VectorDbClient::new(channel))
}

#[tokio::test]
async fn test_grpc_server_start() {
    start_test_server().await.unwrap();
    let mut client = create_test_client().await.unwrap();

    // insert a test vector
    let test_vec = vec![1.0, 2.0, 3.0];

    let mut request = tonic::Request::new(InsertVectorRequest {
        vector: Some(DenseVector {
            values: test_vec.clone(),
        }),
        payload: Some(Struct::default()),
    });
    request
        .metadata_mut()
        .insert("authorization", "Bearer 123".parse().unwrap());
    let status = client.insert_vector(request).await.is_ok();
    assert!(status);
}

#[tokio::test]
async fn test_insert_vector_rpc() {
    start_test_server().await.unwrap();
    let mut client = create_test_client().await.unwrap();

    // insert a test vector
    let test_vec = vec![1.0, 2.0, 3.0];

    let mut request = tonic::Request::new(InsertVectorRequest {
        vector: Some(DenseVector {
            values: test_vec.clone(),
        }),
        payload: Some(Struct::default()),
    });
    request
        .metadata_mut()
        .insert("authorization", "Bearer 123".parse().unwrap());
    let resp = client.insert_vector(request).await;

    // check if request is successful
    assert!(resp.is_ok());

    // check if the vector is actually present in the database
    let mut request = tonic::Request::new(PointId {
        id: resp.unwrap().into_inner().id,
    });
    request
        .metadata_mut()
        .insert("authorization", "Bearer 123".parse().unwrap());
    let resp = client.get_point(request).await;

    // check if request is successful
    assert!(resp.is_ok());
    let point = resp.unwrap().into_inner();
    assert_eq!(point.vector.unwrap().values, test_vec);

    // insert a new vector with mismatched dimensions
    let mut request = tonic::Request::new(InsertVectorRequest {
        vector: Some(DenseVector {
            values: vec![1.0, 2.0],
        }),
        payload: Some(Struct::default()),
    });
    request
        .metadata_mut()
        .insert("authorization", "Bearer 123".parse().unwrap());
    let resp = client.insert_vector(request).await;

    // request must fail
    assert!(resp.is_err());
}

#[tokio::test]
async fn test_delete_vector_rpc() {
    start_test_server().await.unwrap();
    let mut client = create_test_client().await.unwrap();

    // insert a test vector
    let test_vec = vec![1.0, 2.0, 3.0];
    let mut request = tonic::Request::new(InsertVectorRequest {
        vector: Some(DenseVector {
            values: test_vec.clone(),
        }),
        payload: Some(Struct::default()),
    });
    request
        .metadata_mut()
        .insert("authorization", "Bearer 123".parse().unwrap());
    let resp = client.insert_vector(request).await;

    // check if request is successful
    assert!(resp.is_ok());
    let point = resp.unwrap().into_inner();

    // delete the vector
    let mut request = tonic::Request::new(PointId { id: point.id });
    request
        .metadata_mut()
        .insert("authorization", "Bearer 123".parse().unwrap());
    let resp = client.delete_point(request).await;

    // check if request is successful
    assert!(resp.is_ok());

    // verify that the vector is deleted
    let mut request = tonic::Request::new(PointId { id: point.id });
    request
        .metadata_mut()
        .insert("authorization", "Bearer 123".parse().unwrap());
    let resp = client.get_point(request).await;

    // request must fail since the vector is deleted
    assert!(resp.is_err());
}

#[tokio::test]
async fn test_search_vector_rpc() {
    start_test_server().await.unwrap();
    let mut client = create_test_client().await.unwrap();

    // insert a test vector
    let test_vec = vec![1.0, 2.0, 3.0];
    let mut request = tonic::Request::new(InsertVectorRequest {
        vector: Some(DenseVector {
            values: test_vec.clone(),
        }),
        payload: Some(Struct::default()),
    });
    request
        .metadata_mut()
        .insert("authorization", "Bearer 123".parse().unwrap());
    let resp = client.insert_vector(request).await;

    // check if request is successful
    assert!(resp.is_ok());
    let point = resp.unwrap().into_inner();

    let query_vec = vec![2.0, 2.0, 2.0];

    // search for the vector
    let mut request = tonic::Request::new(SearchRequest {
        query_vector: Some(DenseVector {
            values: query_vec.clone(),
        }),
        similarity: 0, // euclidean distance
        limit: 1,
    });
    request
        .metadata_mut()
        .insert("authorization", "Bearer 123".parse().unwrap());
    let resp = client.search_points(request).await;

    // check if request is successful
    assert!(resp.is_ok());
    let result = resp.unwrap().into_inner();

    // 1 vector has to be returned
    assert_eq!(result.result_point_ids.len(), 1);

    // check if the returned point id matches the inserted point id
    assert_eq!(result.result_point_ids[0], PointId { id: point.id });
}

#[tokio::test]
async fn test_unauthorized_rpc() {
    start_test_server().await.unwrap();
    let mut client = create_test_client().await.unwrap();

    // insert a test vector
    let test_vec = vec![1.0, 2.0, 3.0];
    let mut request = tonic::Request::new(InsertVectorRequest {
        vector: Some(DenseVector {
            values: test_vec.clone(),
        }),
        payload: Some(Struct::default()),
    });
    request
        .metadata_mut()
        .insert("authorization", "Bearer 43121".parse().unwrap()); // wrong auth token
    let resp = client.insert_vector(request).await;

    // request must fail
    assert!(resp.is_err());
}
