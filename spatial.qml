StardustClient {
	id: client
	resourcePrefixes: ["res"]
	
	onFrame: function(info) {
		console.log(info);
		gem.setMaterialProperty("color", color(0.0, 0.0, 1.0, sin(info.time)));
		gem.setRotation(<0.0, info.time, 0.0>);
		outerPart.setRotation(<0.0, 0.0, info.time>);
		middlePart.setRotation(<info.time, 0.0, 0.0>);
		innerPart.setRotation(<0.0, 0.0, info.time>);
	}

	Model {
		parent: client.root
		namedParts: [
			ModelPart {
				id: gem
				path: "Gem"
			},
			ModelPart {
				id: innerPart
				path: "OuterPart/MiddlePart/InnerPart"
			},
			ModelPart {
				id: middlePart
				path: "OuterPart/MiddlePart"
			},
			ModelPart {
				id: outerPart
				path: "OuterPart"
			},
		]
	}
}
