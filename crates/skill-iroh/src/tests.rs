#[cfg(test)]
mod device_proto_tests {
    use crate::device_proto::*;

    #[test]
    fn header_all_msg_types() {
        for msg_type in [MSG_SENSOR_CHUNK, MSG_DEVICE_CONNECTED, MSG_DEVICE_DISCONNECTED,
                          MSG_BATTERY, MSG_LOCATION, MSG_META, MSG_PHONE_IMU, MSG_PHONE_INFO] {
            let hdr = encode_header(msg_type, 42, 20260315120000, 0, 100);
            let dec = decode_header(&hdr).unwrap();
            assert_eq!(dec.msg_type, msg_type);
            assert_eq!(dec.seq, 42);
            assert_eq!(dec.payload_len, 100);
        }
    }

    #[test]
    fn header_version_mismatch_returns_none() {
        let mut hdr = encode_header(MSG_SENSOR_CHUNK, 1, 0, 0, 0);
        hdr[0] = 0xFF; // bad version
        assert!(decode_header(&hdr).is_none());
    }

    #[test]
    fn ack_version_mismatch_returns_none() {
        let mut ack = encode_ack(1, ACK_OK);
        ack[0] = 0xFF;
        assert!(decode_ack(&ack).is_none());
    }

    #[test]
    fn sensor_chunk_max_channels() {
        // 24 channels × 1280 samples = 122,880 floats
        let eeg: Vec<Vec<f32>> = (0..24).map(|_| vec![0.1f32; 1280]).collect();
        let raw = encode_sensor_chunk(256.0, &eeg, &[], &[]);
        let dec = decode_sensor_chunk(&raw).unwrap();
        assert_eq!(dec.eeg_data.len(), 24);
        assert_eq!(dec.eeg_data[0].len(), 1280);
        assert_eq!(dec.sample_rate, 256.0);
    }

    #[test]
    fn sensor_chunk_1024_channels() {
        // 1024 channels × 10 samples (small for test speed)
        let eeg: Vec<Vec<f32>> = (0..1024).map(|i| vec![i as f32; 10]).collect();
        let raw = encode_sensor_chunk(500.0, &eeg, &[], &[]);
        let dec = decode_sensor_chunk(&raw).unwrap();
        assert_eq!(dec.eeg_data.len(), 1024);
        assert_eq!(dec.eeg_data[1023][0], 1023.0);
    }

    #[test]
    fn sensor_chunk_2_channels() {
        let eeg = vec![vec![1.0f32; 640], vec![2.0f32; 640]];
        let raw = encode_sensor_chunk(128.0, &eeg, &[], &[]);
        let dec = decode_sensor_chunk(&raw).unwrap();
        assert_eq!(dec.eeg_data.len(), 2);
        assert_eq!(dec.sample_rate, 128.0);
    }

    #[test]
    fn sensor_chunk_full_multimodal() {
        let eeg = vec![vec![1.0f32; 5]; 4];
        let ppg = vec![vec![100.0f64; 2]; 3];
        let imu = vec![(1.0, 2.0, 9.8, 0.1, 0.2, 0.3)];
        let raw = encode_sensor_chunk(256.0, &eeg, &ppg, &imu);
        let dec = decode_sensor_chunk(&raw).unwrap();
        assert_eq!(dec.eeg_data.len(), 4);
        assert_eq!(dec.ppg_data.len(), 3);
        assert_eq!(dec.imu_data.len(), 1);
        assert!((dec.imu_data[0].2 - 9.8).abs() < 1e-5);
    }

    #[test]
    fn sensor_chunk_truncated_error() {
        let raw = encode_sensor_chunk(256.0, &vec![vec![1.0; 3]; 2], &[], &[]);
        // Truncate
        assert!(decode_sensor_chunk(&raw[..raw.len() - 5]).is_err());
    }

    #[test]
    fn location_extreme_values() {
        let loc = Location {
            latitude: -90.0, longitude: 180.0, altitude: -430.0,
            accuracy: 0.5, speed: -1.0, heading: -1.0,
        };
        let raw = encode_location(&loc);
        let dec = decode_location(&raw).unwrap();
        assert_eq!(dec.latitude, -90.0);
        assert_eq!(dec.longitude, 180.0);
        assert_eq!(dec.altitude, -430.0);
    }

    #[test]
    fn phone_imu_empty_batch() {
        let raw = encode_phone_imu(&[]);
        let dec = decode_phone_imu(&raw).unwrap();
        assert!(dec.is_empty());
    }

    #[test]
    fn phone_imu_large_batch() {
        let samples: Vec<PhoneImuSample> = (0..500).map(|i| PhoneImuSample {
            dt: i as f32 * 0.02,
            raw_accel: [0.0, 0.0, -1.0],
            user_accel: [0.0; 3],
            gravity: [0.0, 0.0, -1.0],
            gyro: [0.0; 3],
            mag: [25.0, -10.0, 42.0],
            attitude: [0.0; 3],
            pressure: 101.3,
            rel_altitude: 0.0,
            ambient_light: 0.5,
            proximity: 0.0,
        }).collect();
        let raw = encode_phone_imu(&samples);
        let dec = decode_phone_imu(&raw).unwrap();
        assert_eq!(dec.len(), 500);
        assert!((dec[499].dt - 9.98).abs() < 0.01);
    }

    #[test]
    fn battery_roundtrip_edge_cases() {
        for val in [0.0f32, 100.0, 50.5, f32::NAN, f32::INFINITY] {
            let raw = encode_battery(val);
            let dec = decode_battery(&raw).unwrap();
            if val.is_nan() {
                assert!(dec.is_nan());
            } else {
                assert_eq!(dec, val);
            }
        }
    }
}

#[cfg(test)]
mod receiver_tests {
    use crate::device_receiver::*;

    #[test]
    fn event_channel_capacity() {
        let (tx, _rx) = event_channel();
        // Should be able to send up to capacity without blocking
        for i in 0..16 {
            tx.try_send(RemoteDeviceEvent::Battery {
                seq: i, timestamp: 0, level_pct: 50.0,
            }).expect("should not be full");
        }
        // 17th should fail (capacity=16)
        assert!(tx.try_send(RemoteDeviceEvent::Battery {
            seq: 17, timestamp: 0, level_pct: 50.0,
        }).is_err());
    }
}
