from app import message


def test_message() -> None:
    assert message() == "fixture"
