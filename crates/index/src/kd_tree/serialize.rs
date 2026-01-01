use super::types::{KDTreeNode};
use super::index::KDTree;
use crate::SerializableIndexer;


impl SerializableIndexer for KDTree {
	fn serialize_topology(&self) -> Vec<u8> {
    	let mut buffer = Vec::new();
     	self.serialize_topology_recursive(&self.root, &mut buffer);
     	buffer
	}
}


impl KDTree {

    fn serialize_topology_recursive(&self,  current_opt: &Option<Box<KDTreeNode>>, buffer: &mut Vec<u8>) {
		if let Some(current) = current_opt {
			// push marker byte
		  	buffer.push(1u8);

			let uuid_bytes = current.indexed_vector.id.to_bytes_le();
			buffer.extend_from_slice(&uuid_bytes);

			// serialize left subtree topology
			self.serialize_topology_recursive(&current.left, buffer);
			// serialize right subtree topology
			self.serialize_topology_recursive(&current.right, buffer);
		} else {
			// push skip marker byte
		  	buffer.push(0u8);
		}
    }
}
