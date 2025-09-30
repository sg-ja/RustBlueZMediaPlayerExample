
#[derive(Debug, Default)]
pub struct Player {
    browsable:  bool,
    name:       String,
    position:   u64,
    repeat:     bool,
    shuffle:    bool,
    track:      String,
}