import test
import testy

StardustClient {
	id: test
	test_int: 128
	test_float: 1.0
	test_string: "test"

	Model {
		id: model
		model: "asteroids:test"
		rotation: <0.0, 0.0, 1.0>
	}
}
